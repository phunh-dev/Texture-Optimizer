// Texture Optimizer - UV Remap Data importer for Unity (Editor only).
//
// Put this file in any "Editor" folder of your Unity project. For every model
// (FBX/OBJ/DAE) that has a sidecar "<ModelName>.uvremap.json" in the same
// folder, the UVs of each remapped material's submeshes are rewritten at
// import time:  uv' = offset + normalize(uv) * scale
// so the model samples the shared texture atlas. The model file itself is
// never modified. Re-import the model after adding/changing the sidecar
// (newer Unity versions re-import automatically through the dependency).
//
// JSON contract (written by texopt-core mesh::remap_json, version 1):
//   models[].model                  model file name, matched case-insensitively
//   models[].mergedMaterialName     optional, informational
//   models[].materials[]            one entry per source material:
//     materialIndex, materialName   matched against the renderer's material name
//     skipped                       true -> leave this material untouched
//     atlasPage, uvChannel          atlas page index, Unity UV channel (0 = uv)
//     offset[2], scale[2]           atlas transform (bottom-left UV origin,
//                                   same convention as Unity meshes)
//     normalize                     "none" | "clamp" | "wrap" | "repeat"
//     repeatOrigin[2], repeatTiles[2]  used by "repeat": n = (uv - origin) / tiles
//     originalTextures[]            {channel, path}, informational
//
// Weld Vertices is turned off for these models so vertices are never shared
// between submeshes with different materials (each would need its own UV).
#if UNITY_EDITOR
using System;
using System.Collections.Generic;
using System.IO;
using UnityEditor;
using UnityEngine;

public class TextureOptimizerUvRemapPostprocessor : AssetPostprocessor
{
    const int SupportedVersion = 1;
    const float Epsilon = 1e-4f;

    // DTO fields are assigned by JsonUtility via reflection.
#pragma warning disable 0649
    [Serializable] class ChannelPath { public string channel; public string path; }

    [Serializable]
    class MaterialEntry
    {
        public int materialIndex;
        public string materialName;
        public bool skipped;
        public int atlasPage;
        public int uvChannel;
        public float[] offset;
        public float[] scale;
        public string normalize;
        public int[] repeatOrigin;
        public int[] repeatTiles;
        public ChannelPath[] originalTextures;
    }

    [Serializable]
    class ModelEntry
    {
        public string model;
        public string mergedMaterialName;
        public MaterialEntry[] materials;
    }

    [Serializable]
    class RemapFile
    {
        public int version;
        public string generator;
        public string uvOrigin;
        public ModelEntry[] models;
    }
#pragma warning restore 0649

    public override uint GetVersion() { return 1; }

    static string SidecarPath(string modelPath)
    {
        string dir = Path.GetDirectoryName(modelPath) ?? "";
        return Path.Combine(dir, Path.GetFileNameWithoutExtension(modelPath) + ".uvremap.json").Replace('\\', '/');
    }

    void OnPreprocessModel()
    {
        string sidecar = SidecarPath(assetPath);
        if (!File.Exists(sidecar)) return;
#if UNITY_2020_2_OR_NEWER
        context.DependsOnSourceAsset(sidecar);
#endif
        var importer = assetImporter as ModelImporter;
        if (importer != null) importer.weldVertices = false;
    }

    void OnPostprocessModel(GameObject root)
    {
        string sidecar = SidecarPath(assetPath);
        if (!File.Exists(sidecar)) return;

        RemapFile file;
        try { file = JsonUtility.FromJson<RemapFile>(File.ReadAllText(sidecar)); }
        catch (Exception e) { Debug.LogError("[TextureOptimizer] cannot read " + sidecar + ": " + e.Message); return; }
        if (file == null || file.models == null || file.models.Length == 0) return;
        if (file.version != SupportedVersion)
        {
            Debug.LogError("[TextureOptimizer] unsupported uvremap version " + file.version + " in " + sidecar);
            return;
        }

        string fileName = Path.GetFileName(assetPath);
        ModelEntry entry = null;
        foreach (var m in file.models)
            if (string.Equals(m.model, fileName, StringComparison.OrdinalIgnoreCase)) { entry = m; break; }
        if (entry == null && file.models.Length == 1) entry = file.models[0];
        if (entry == null || entry.materials == null) return;

        var byName = new Dictionary<string, MaterialEntry>();
        foreach (var mat in entry.materials)
            if (!mat.skipped && mat.materialName != null && !byName.ContainsKey(mat.materialName)) byName[mat.materialName] = mat;

        var done = new HashSet<Mesh>();
        int remapped = 0, conflicts = 0;
        foreach (var renderer in root.GetComponentsInChildren<Renderer>(true))
        {
            Mesh mesh = null;
            var smr = renderer as SkinnedMeshRenderer;
            if (smr != null) mesh = smr.sharedMesh;
            else
            {
                var mf = renderer.GetComponent<MeshFilter>();
                if (mf != null) mesh = mf.sharedMesh;
            }
            if (mesh == null || !done.Add(mesh)) continue;
            remapped += RemapMesh(mesh, renderer.sharedMaterials, byName, ref conflicts);
        }

        if (conflicts > 0)
            Debug.LogWarning("[TextureOptimizer] " + fileName + ": " + conflicts + " vertices are shared by submeshes with different remaps; they keep the first remap.");
        Debug.Log("[TextureOptimizer] " + fileName + ": remapped UVs of " + remapped + " submeshes using " + Path.GetFileName(sidecar));
    }

    static string CleanName(Material m)
    {
        if (m == null) return null;
        string n = m.name;
        const string suffix = " (Instance)";
        return n.EndsWith(suffix) ? n.Substring(0, n.Length - suffix.Length) : n;
    }

    static int RemapMesh(Mesh mesh, Material[] materials, Dictionary<string, MaterialEntry> byName, ref int conflicts)
    {
        var uvSets = new Dictionary<int, List<Vector2>>();
        var owner = new Dictionary<int, MaterialEntry[]>();
        int count = 0;
        for (int s = 0; s < mesh.subMeshCount && s < materials.Length; s++)
        {
            string name = CleanName(materials[s]);
            MaterialEntry e;
            if (name == null || !byName.TryGetValue(name, out e)) continue;
            if (e.uvChannel < 0 || e.uvChannel > 7) continue;

            List<Vector2> uvs;
            if (!uvSets.TryGetValue(e.uvChannel, out uvs))
            {
                uvs = new List<Vector2>();
                mesh.GetUVs(e.uvChannel, uvs);
                if (uvs.Count != mesh.vertexCount) continue; // channel missing
                uvSets[e.uvChannel] = uvs;
                owner[e.uvChannel] = new MaterialEntry[mesh.vertexCount];
            }
            MaterialEntry[] own = owner[e.uvChannel];
            var source = new List<Vector2>(uvs); // remap from the original values

            int[] indices = mesh.GetIndices(s);
            int stride = mesh.GetTopology(s) == MeshTopology.Triangles ? 3 : (mesh.GetTopology(s) == MeshTopology.Quads ? 4 : 1);
            for (int f = 0; f + stride <= indices.Length; f += stride)
            {
                Vector2 tile = Vector2.zero;
                if (e.normalize == "wrap")
                {
                    Vector2 min = new Vector2(float.MaxValue, float.MaxValue);
                    for (int k = 0; k < stride; k++) min = Vector2.Min(min, source[indices[f + k]]);
                    tile = new Vector2(Mathf.Floor(min.x + Epsilon), Mathf.Floor(min.y + Epsilon));
                }
                for (int k = 0; k < stride; k++)
                {
                    int v = indices[f + k];
                    if (own[v] != null) { if (own[v] != e) conflicts++; continue; }
                    own[v] = e;
                    uvs[v] = Apply(e, source[v], tile);
                }
            }
            count++;
        }
        foreach (var kv in uvSets) mesh.SetUVs(kv.Key, kv.Value);
        return count;
    }

    static Vector2 Apply(MaterialEntry e, Vector2 uv, Vector2 wrapTile)
    {
        Vector2 n = uv;
        switch (e.normalize)
        {
            case "clamp": n = new Vector2(Mathf.Clamp01(uv.x), Mathf.Clamp01(uv.y)); break;
            case "wrap": n = uv - wrapTile; break;
            case "repeat":
                n = new Vector2((uv.x - e.repeatOrigin[0]) / Mathf.Max(1, e.repeatTiles[0]),
                                (uv.y - e.repeatOrigin[1]) / Mathf.Max(1, e.repeatTiles[1]));
                break;
        }
        return new Vector2(e.offset[0] + n.x * e.scale[0], e.offset[1] + n.y * e.scale[1]);
    }
}
#endif
