use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use chrono::TimeZone;
use serde_json::json;
use texopt_core::rename::{
    CaseMode, Conflict, EnginePreset, ExtensionCase, FindReplace, RENAME_INVALID_DATE_FORMAT,
    RENAME_INVALID_REGEX, RENAME_UNKNOWN_TOKEN, RenameEntry, RenameParams, RenamePlanItem,
    SmartParams, SortBy, TextureType, apply_case, detect_texture_type, plan,
};

fn entry(path: &str) -> RenameEntry {
    RenameEntry {
        path: PathBuf::from(path),
        width: 64,
        height: 32,
        modified_ms: 0,
        size_bytes: 100,
    }
}

fn no_disk(_: &Path) -> bool {
    false
}

fn run(entries: &[RenameEntry], params: &RenameParams) -> Vec<RenamePlanItem> {
    plan(entries, params, &no_disk).expect("plan succeeds")
}

fn names(items: &[RenamePlanItem]) -> Vec<String> {
    items
        .iter()
        .map(|i| i.to.file_name().unwrap().to_string_lossy().into_owned())
        .collect()
}

fn one(path: &str, params: &RenameParams) -> String {
    names(&run(&[entry(path)], params)).remove(0)
}

fn tpl(template: &str) -> RenameParams {
    RenameParams {
        template: template.into(),
        ..RenameParams::default()
    }
}

// ---------- tokens ----------

#[test]
fn default_params_keep_names() {
    let items = run(
        &[entry("tex/rock.png"), entry("tex/Grass 01.TGA")],
        &RenameParams::default(),
    );
    assert_eq!(names(&items), ["rock.png", "Grass 01.TGA"]);
    assert!(items.iter().all(|i| i.conflict.is_none()));
    assert_eq!(items[0].from, PathBuf::from("tex/rock.png"));
    assert_eq!(items[0].to, Path::new("tex").join("rock.png"));
}

#[test]
fn index_token_with_start_step_and_zero_pad() {
    let entries = [entry("a/x.png"), entry("a/y.png"), entry("a/z.png")];
    let p = RenameParams {
        start_number: 5,
        step: 10,
        zero_pad: 3,
        ..tpl("img_{index}")
    };
    assert_eq!(
        names(&run(&entries, &p)),
        ["img_005.png", "img_015.png", "img_025.png"]
    );
    let p = RenameParams {
        start_number: 0,
        step: 1,
        zero_pad: 0,
        ..tpl("{index}")
    };
    assert_eq!(names(&run(&entries, &p)), ["0.png", "1.png", "2.png"]);
    let p = RenameParams {
        start_number: -1,
        step: 1,
        zero_pad: 2,
        ..tpl("n{index}")
    };
    assert_eq!(
        names(&run(&entries, &p)),
        ["n-01.png", "n00.png", "n01.png"]
    );
}

#[test]
fn parent_width_height_ext_tokens() {
    assert_eq!(
        one("assets/textures/rock.png", &tpl("{parent}_{name}")),
        "textures_rock.png"
    );
    assert_eq!(
        one("assets/textures/rock.png", &tpl("{name}_{width}x{height}")),
        "rock_64x32.png"
    );
    assert_eq!(
        one("assets/textures/rock.png", &tpl("{name}_{ext}")),
        "rock_png.png"
    );
    let no_ext = RenameParams {
        keep_extension: false,
        ..tpl("{name}.{ext}.bak")
    };
    assert_eq!(one("assets/textures/rock.png", &no_ext), "rock.png.bak");
}

#[test]
fn date_token_uses_modified_time_and_format() {
    let ms = 1_700_000_000_000i64;
    let expected = chrono::Local
        .timestamp_millis_opt(ms)
        .unwrap()
        .format("%Y-%m-%d")
        .to_string();
    let mut e = entry("a/rock.png");
    e.modified_ms = ms;
    let p = RenameParams {
        date_format: "%Y-%m-%d".into(),
        ..tpl("{name}_{date}")
    };
    assert_eq!(
        names(&run(&[e.clone()], &p)),
        [format!("rock_{expected}.png")]
    );
    // default format %Y%m%d
    let compact = chrono::Local
        .timestamp_millis_opt(ms)
        .unwrap()
        .format("%Y%m%d")
        .to_string();
    assert_eq!(
        names(&run(&[e.clone()], &tpl("{date}"))),
        [format!("{compact}.png")]
    );

    let bad = RenameParams {
        date_format: "%Q".into(),
        ..tpl("{date}")
    };
    assert_eq!(
        plan(&[e], &bad, &no_disk).unwrap_err().code,
        RENAME_INVALID_DATE_FORMAT
    );
}

#[test]
fn type_token_uses_preset_suffix() {
    let unreal = tpl("{name}_{type}");
    assert_eq!(one("a/rock_normal.png", &unreal), "rock_normal_N.png");
    let unity = RenameParams {
        smart: SmartParams {
            preset: EnginePreset::Unity,
            ..SmartParams::default()
        },
        ..unreal
    };
    assert_eq!(one("a/rock_normal.png", &unity), "rock_normal_Normal.png");
    // No detected type -> empty.
    assert_eq!(one("a/rock.png", &tpl("{name}{type}")), "rock.png");
}

#[test]
fn unknown_token_is_an_error() {
    let err = plan(&[entry("a/b.png")], &tpl("{nope}"), &no_disk).unwrap_err();
    assert_eq!(err.code, RENAME_UNKNOWN_TOKEN);
    assert_eq!(err.params["token"], json!("nope"));
    // An unmatched brace is literal.
    assert_eq!(one("a/b.png", &tpl("{name")), "{name.png");
}

#[test]
fn prefix_and_suffix() {
    let p = RenameParams {
        prefix: "pre_".into(),
        suffix: "_suf".into(),
        ..RenameParams::default()
    };
    assert_eq!(one("a/rock.png", &p), "pre_rock_suf.png");
}

// ---------- case ----------

#[test]
fn case_transforms() {
    let s = "myXMLFile_v2 final-Copy";
    assert_eq!(apply_case(s, CaseMode::Keep), s);
    assert_eq!(apply_case(s, CaseMode::Lower), "myxmlfile_v2 final-copy");
    assert_eq!(apply_case(s, CaseMode::Upper), "MYXMLFILE_V2 FINAL-COPY");
    assert_eq!(apply_case(s, CaseMode::Snake), "my_xml_file_v2_final_copy");
    assert_eq!(apply_case(s, CaseMode::Kebab), "my-xml-file-v2-final-copy");
    assert_eq!(apply_case(s, CaseMode::Camel), "myXmlFileV2FinalCopy");
    assert_eq!(apply_case(s, CaseMode::Pascal), "MyXmlFileV2FinalCopy");
}

#[test]
fn case_transforms_with_digits_and_separators() {
    let s = "Rock 01__albedo.final";
    assert_eq!(apply_case(s, CaseMode::Snake), "rock_01_albedo_final");
    assert_eq!(apply_case(s, CaseMode::Kebab), "rock-01-albedo-final");
    assert_eq!(apply_case(s, CaseMode::Camel), "rock01AlbedoFinal");
    assert_eq!(apply_case(s, CaseMode::Pascal), "Rock01AlbedoFinal");
    assert_eq!(apply_case("texture2D", CaseMode::Snake), "texture2_d");
    assert_eq!(apply_case("  --  ", CaseMode::Snake), "");
}

#[test]
fn case_transforms_keep_vietnamese_diacritics() {
    let s = "Đá Xanh lá_Cây 01";
    assert_eq!(apply_case(s, CaseMode::Keep), s);
    assert_eq!(apply_case(s, CaseMode::Lower), "đá xanh lá_cây 01");
    assert_eq!(apply_case(s, CaseMode::Upper), "ĐÁ XANH LÁ_CÂY 01");
    assert_eq!(apply_case(s, CaseMode::Snake), "đá_xanh_lá_cây_01");
    assert_eq!(apply_case(s, CaseMode::Kebab), "đá-xanh-lá-cây-01");
    assert_eq!(apply_case(s, CaseMode::Camel), "đáXanhLáCây01");
    assert_eq!(apply_case(s, CaseMode::Pascal), "ĐáXanhLáCây01");
}

#[test]
fn case_applies_to_name_not_extension_and_extension_case() {
    let p = RenameParams {
        case: CaseMode::Upper,
        ..RenameParams::default()
    };
    assert_eq!(one("a/rock.png", &p), "ROCK.png");
    let p = RenameParams {
        extension_case: ExtensionCase::Lower,
        ..RenameParams::default()
    };
    assert_eq!(one("a/Rock.PNG", &p), "Rock.png");
    let p = RenameParams {
        extension_case: ExtensionCase::Upper,
        ..RenameParams::default()
    };
    assert_eq!(one("a/Rock.png", &p), "Rock.PNG");
}

// ---------- find / replace ----------

fn fr(find: &str, replace: &str, regex: bool, case_sensitive: bool) -> FindReplace {
    FindReplace {
        find: find.into(),
        replace: replace.into(),
        regex,
        case_sensitive,
    }
}

#[test]
fn literal_find_replace_in_order_and_case_sensitivity() {
    let p = RenameParams {
        find_replace: vec![fr("a", "b", false, true), fr("b", "c", false, true)],
        ..RenameParams::default()
    };
    assert_eq!(one("x/aXa.png", &p), "cXc.png");
    let insensitive = RenameParams {
        find_replace: vec![fr("ROCK", "stone", false, false)],
        ..RenameParams::default()
    };
    assert_eq!(one("x/Rock_01.png", &insensitive), "stone_01.png");
    let sensitive = RenameParams {
        find_replace: vec![fr("ROCK", "stone", false, true)],
        ..RenameParams::default()
    };
    assert_eq!(one("x/Rock_01.png", &sensitive), "Rock_01.png");
    // Literal mode does not expand `$1` or treat `.` as a wildcard.
    let lit = RenameParams {
        find_replace: vec![fr(".", "$1", false, true)],
        ..RenameParams::default()
    };
    assert_eq!(one("x/a.b.png", &lit), "a$1b.png");
}

#[test]
fn regex_find_replace_with_capture_groups() {
    let p = RenameParams {
        find_replace: vec![fr(r"(\w+?)_(\d+)", "${2}_$1", true, true)],
        ..RenameParams::default()
    };
    assert_eq!(one("x/rock_01.png", &p), "01_rock.png");
    let named = RenameParams {
        find_replace: vec![fr(
            r"(?P<base>[a-z]+)(?P<num>\d+)",
            "$num-$base",
            true,
            false,
        )],
        ..RenameParams::default()
    };
    assert_eq!(one("x/GRASS7.png", &named), "7-GRASS.png");
}

#[test]
fn invalid_regex_reports_code_and_params() {
    let p = RenameParams {
        find_replace: vec![fr("ok", "x", true, true), fr("(unclosed", "x", true, true)],
        ..RenameParams::default()
    };
    let err = plan(&[entry("a/b.png")], &p, &no_disk).unwrap_err();
    assert_eq!(err.code, RENAME_INVALID_REGEX);
    assert_eq!(err.params["pattern"], json!("(unclosed"));
    assert_eq!(err.params["index"], json!(1));
}

// ---------- smart naming ----------

#[test]
fn detects_every_texture_type_keyword() {
    use TextureType::*;
    let cases: &[(&str, TextureType)] = &[
        ("rock_albedo", BaseColor),
        ("rock_diffuse", BaseColor),
        ("rock_basecolor", BaseColor),
        ("rock_base_color", BaseColor),
        ("RockBaseColor", BaseColor),
        ("rock_color", BaseColor),
        ("rock_col", BaseColor),
        ("rock_normal", Normal),
        ("rock_nrm", Normal),
        ("rock_nor", Normal),
        ("RockNorm", Normal),
        ("rock_roughness", Roughness),
        ("rock-rough", Roughness),
        ("rock_rgh", Roughness),
        ("rock_metallic", Metallic),
        ("rock metal", Metallic),
        ("rock_mtl", Metallic),
        ("rock_ao", Ao),
        ("rockAO", Ao),
        ("rock_occlusion", Ao),
        ("rock_emissive", Emissive),
        ("rock_emission", Emissive),
        ("rock_emit", Emissive),
        ("rock_height", Height),
        ("rock_disp", Height),
        ("rock_displacement", Height),
        ("rock_mask", Mask),
        ("rock_msk", Mask),
        ("rock_ORM", Orm),
        ("rock_opacity", Opacity),
        ("rock_alpha", Opacity),
        ("Rock_Normal_4k", Normal),
    ];
    for (stem, ty) in cases {
        assert_eq!(detect_texture_type(stem), Some(*ty), "{stem}");
    }
    for stem in ["rock", "colorado", "normality", "rock_01", ""] {
        assert_eq!(detect_texture_type(stem), None, "{stem}");
    }
}

fn smart(preset: EnginePreset) -> RenameParams {
    RenameParams {
        smart: SmartParams {
            enabled: true,
            preset,
            custom_map: BTreeMap::new(),
        },
        ..RenameParams::default()
    }
}

#[test]
fn smart_unreal_preset_for_every_type() {
    let p = smart(EnginePreset::Unreal);
    let cases = [
        ("Rock_BaseColor", "T_Rock_D"),
        ("Rock_Normal", "T_Rock_N"),
        ("Rock_Roughness", "T_Rock_R"),
        ("Rock_Metallic", "T_Rock_M"),
        ("Rock_AO", "T_Rock_AO"),
        ("Rock_Emissive", "T_Rock_E"),
        ("Rock_Height", "T_Rock_H"),
        ("Rock_Mask", "T_Rock_M"),
        ("Rock_ORM", "T_Rock_ORM"),
        ("Rock_Opacity", "T_Rock_O"),
        ("RockNormal", "T_Rock_N"),
        ("Normal_Rock", "T_Rock_N"),
        ("Rock_Normal_2", "T_Rock_2_N"),
        ("T_Rock_Normal", "T_Rock_N"),
        ("Rock", "T_Rock"),
    ];
    for (stem, want) in cases {
        assert_eq!(
            one(&format!("a/{stem}.png"), &p),
            format!("{want}.png"),
            "{stem}"
        );
    }
}

#[test]
fn smart_unity_godot_and_custom_presets() {
    let unity = smart(EnginePreset::Unity);
    assert_eq!(one("a/rock_diffuse.png", &unity), "rock_Albedo.png");
    assert_eq!(one("a/rock_nrm.png", &unity), "rock_Normal.png");
    assert_eq!(one("a/rock_ao.png", &unity), "rock_Occlusion.png");
    assert_eq!(one("a/rock_emit.png", &unity), "rock_Emission.png");
    assert_eq!(one("a/rock.png", &unity), "rock.png");

    let godot = smart(EnginePreset::Godot);
    assert_eq!(one("a/RockBaseColor.png", &godot), "rock_albedo.png");
    assert_eq!(
        one("a/Mossy Rock-Roughness.png", &godot),
        "mossy_rock_roughness.png"
    );
    assert_eq!(one("a/Rock_ORM.png", &godot), "rock_orm.png");
    assert_eq!(one("a/Rock Big.png", &godot), "rock_big.png");

    let mut custom = smart(EnginePreset::Custom);
    custom
        .smart
        .custom_map
        .insert(TextureType::Normal, "nrml".into());
    assert_eq!(one("a/Rock_Normal.png", &custom), "Rock_nrml.png");
    // Unmapped types keep their name untouched.
    assert_eq!(one("a/Rock_Albedo.png", &custom), "Rock_Albedo.png");
}

#[test]
fn smart_with_explicit_type_token_does_not_duplicate_suffix() {
    let p = RenameParams {
        template: "{type}_{name}".into(),
        ..smart(EnginePreset::Unreal)
    };
    assert_eq!(one("a/Rock_Normal.png", &p), "T_N_Rock.png");
}

#[test]
fn smart_runs_after_case_and_affixes() {
    let p = RenameParams {
        case: CaseMode::Pascal,
        prefix: "env ".into(),
        ..smart(EnginePreset::Unreal)
    };
    assert_eq!(one("a/mossy_rock_normal.png", &p), "T_EnvMossyRock_N.png");
}

// ---------- sorting ----------

fn sort_entries() -> Vec<RenameEntry> {
    let mk = |name: &str, w: u32, h: u32, m: i64, s: u64| RenameEntry {
        path: PathBuf::from(format!("d/{name}.png")),
        width: w,
        height: h,
        modified_ms: m,
        size_bytes: s,
    };
    vec![
        mk("b10", 8, 8, 300, 5),
        mk("B2", 32, 32, 100, 50),
        mk("a1", 16, 16, 200, 1),
    ]
}

fn sorted_names(sort_by: SortBy, desc: bool) -> Vec<String> {
    let p = RenameParams {
        sort_by,
        sort_desc: desc,
        ..tpl("{index}_{name}")
    };
    names(&run(&sort_entries(), &p))
}

#[test]
fn sort_modes_drive_numbering() {
    assert_eq!(
        sorted_names(SortBy::None, false),
        ["1_b10.png", "2_B2.png", "3_a1.png"]
    );
    assert_eq!(
        sorted_names(SortBy::Name, false),
        ["1_a1.png", "2_b10.png", "3_B2.png"]
    );
    assert_eq!(
        sorted_names(SortBy::NaturalName, false),
        ["1_a1.png", "2_B2.png", "3_b10.png"]
    );
    assert_eq!(
        sorted_names(SortBy::NaturalName, true),
        ["1_b10.png", "2_B2.png", "3_a1.png"]
    );
    assert_eq!(
        sorted_names(SortBy::Modified, false),
        ["1_B2.png", "2_a1.png", "3_b10.png"]
    );
    assert_eq!(
        sorted_names(SortBy::Size, false),
        ["1_a1.png", "2_b10.png", "3_B2.png"]
    );
    assert_eq!(
        sorted_names(SortBy::Size, true),
        ["1_B2.png", "2_b10.png", "3_a1.png"]
    );
    assert_eq!(
        sorted_names(SortBy::Dimensions, false),
        ["1_b10.png", "2_a1.png", "3_B2.png"]
    );
}

// ---------- conflicts ----------

#[test]
fn duplicate_in_batch_is_flagged_on_all_members() {
    let items = run(
        &[entry("a/x.png"), entry("a/y.png"), entry("b/z.png")],
        &tpl("same"),
    );
    assert_eq!(items[0].conflict, Some(Conflict::DuplicateInBatch));
    assert_eq!(items[1].conflict, Some(Conflict::DuplicateInBatch));
    assert_eq!(items[2].conflict, None, "different folder, no clash");
    // Case-insensitive file systems: SAME.png and same.png clash.
    let p = RenameParams {
        find_replace: vec![fr("^x$", "SAME", true, true), fr("^y$", "same", true, true)],
        ..RenameParams::default()
    };
    let items = run(&[entry("a/x.png"), entry("a/y.png")], &p);
    assert!(
        items
            .iter()
            .all(|i| i.conflict == Some(Conflict::DuplicateInBatch))
    );
}

#[test]
fn exists_on_disk_is_flagged_unless_moved_away_in_batch() {
    let on_disk = |p: &Path| {
        ["a/new.png", "a/x.png", "a/y.png"]
            .iter()
            .any(|d| p == Path::new("a").join(&d[2..]))
    };
    let items = plan(&[entry("a/old.png")], &tpl("new"), &on_disk).unwrap();
    assert_eq!(items[0].conflict, Some(Conflict::ExistsOnDisk));

    // Swap x <-> y: both targets exist but are being renamed away.
    let swap = RenameParams {
        find_replace: vec![
            fr("^x$", "tmp", true, true),
            fr("^y$", "x", true, true),
            fr("^tmp$", "y", true, true),
        ],
        ..RenameParams::default()
    };
    let items = plan(&[entry("a/x.png"), entry("a/y.png")], &swap, &on_disk).unwrap();
    assert_eq!(names(&items), ["y.png", "x.png"]);
    assert!(items.iter().all(|i| i.conflict.is_none()), "{items:?}");

    // Unchanged name is never a conflict even though it exists.
    let items = plan(&[entry("a/x.png")], &RenameParams::default(), &on_disk).unwrap();
    assert_eq!(items[0].conflict, None);
}

#[test]
fn invalid_names_on_all_platforms() {
    let bad_templates = [
        "a<b", "a>b", "a:b", "a\"b", "a/b", "a\\b", "a|b", "a?b", "a*b", "a\u{1}b", "a\u{7f}b", "",
        "CON", "con", "Prn", "AUX", "nul", "COM1", "com9", "LPT1", "lpt9",
    ];
    for t in bad_templates {
        let items = run(&[entry("d/x.png")], &tpl(t));
        assert_eq!(items[0].conflict, Some(Conflict::InvalidName), "{t:?}");
    }
    // Trailing dot / space only matter at the very end of the file name.
    for t in ["name.", "name ", "..", "."] {
        let p = RenameParams {
            keep_extension: false,
            ..tpl(t)
        };
        assert_eq!(
            run(&[entry("d/x.png")], &p)[0].conflict,
            Some(Conflict::InvalidName),
            "{t:?}"
        );
    }
    let too_long = "a".repeat(260);
    assert_eq!(
        run(&[entry("d/x.png")], &tpl(&too_long))[0].conflict,
        Some(Conflict::InvalidName)
    );
    for ok in ["CONSOLE", "COM10", "com", "a.b c", "Đá xanh", "name_"] {
        assert_eq!(
            run(&[entry("d/x.png")], &tpl(ok))[0].conflict,
            None,
            "{ok:?}"
        );
    }
}

#[test]
fn sanitize_fixes_invalid_names() {
    let san = |t: &str, keep_ext: bool, repl: &str| {
        let p = RenameParams {
            sanitize: true,
            invalid_char_replacement: repl.into(),
            keep_extension: keep_ext,
            ..tpl(t)
        };
        let items = run(&[entry("d/x.png")], &p);
        assert_eq!(items[0].conflict, None, "{t:?}");
        names(&items).remove(0)
    };
    assert_eq!(san("a:b*c", true, "_"), "a_b_c.png");
    assert_eq!(san("a/b", true, "-"), "a-b.png");
    assert_eq!(san("con", true, "_"), "con_.png");
    assert_eq!(san("LPT1", true, "_"), "LPT1_.png");
    assert_eq!(san("name. ", false, "_"), "name");
    assert_eq!(san("", true, "_"), "_.png");
    assert_eq!(san("a\u{1}b", true, ""), "ab.png");

    let bad = RenameParams {
        sanitize: true,
        invalid_char_replacement: "?".into(),
        ..tpl("a:b")
    };
    assert_eq!(
        plan(&[entry("d/x.png")], &bad, &no_disk).unwrap_err().code,
        "INVALID_PARAMS"
    );
}

// ---------- serde contract ----------

#[test]
fn serde_contract_is_camel_case() {
    let p: RenameParams = serde_json::from_str("{}").unwrap();
    assert_eq!(p, RenameParams::default());
    assert_eq!(p.template, "{name}");
    assert!(p.keep_extension);
    assert_eq!(p.start_number, 1);
    assert_eq!(p.step, 1);
    assert_eq!(p.date_format, "%Y%m%d");
    assert_eq!(p.invalid_char_replacement, "_");

    let v = serde_json::to_value(RenameParams::default()).unwrap();
    for key in [
        "template",
        "prefix",
        "suffix",
        "startNumber",
        "step",
        "zeroPad",
        "case",
        "findReplace",
        "smart",
        "sortBy",
        "sortDesc",
        "keepExtension",
        "extensionCase",
        "dateFormat",
        "sanitize",
        "invalidCharReplacement",
    ] {
        assert!(v.get(key).is_some(), "missing {key}");
    }
    assert_eq!(
        v["smart"],
        json!({"enabled": false, "preset": "unreal", "customMap": {}})
    );

    let p: RenameParams = serde_json::from_value(json!({
        "case": "pascal", "sortBy": "naturalName", "extensionCase": "upper",
        "findReplace": [{"find": "a", "replace": "b", "regex": true}],
        "smart": {"enabled": true, "preset": "custom", "customMap": {"baseColor": "BC", "ao": "Occ"}}
    }))
    .unwrap();
    assert_eq!(p.case, CaseMode::Pascal);
    assert_eq!(p.sort_by, SortBy::NaturalName);
    assert_eq!(p.extension_case, ExtensionCase::Upper);
    assert!(
        p.find_replace[0].case_sensitive,
        "caseSensitive defaults to true"
    );
    assert_eq!(p.smart.custom_map[&TextureType::BaseColor], "BC");
    assert_eq!(p.smart.custom_map[&TextureType::Ao], "Occ");

    let items = run(&[entry("a/x.png"), entry("a/y.png")], &tpl("same"));
    let v = serde_json::to_value(&items[0]).unwrap();
    assert_eq!(v["conflict"], "duplicateInBatch");
    assert!(v.get("from").is_some() && v.get("to").is_some());
    let e: RenameEntry = serde_json::from_value(json!({
        "path": "a/b.png", "width": 1, "height": 2, "modifiedMs": 3, "sizeBytes": 4
    }))
    .unwrap();
    assert_eq!(e.size_bytes, 4);
}
