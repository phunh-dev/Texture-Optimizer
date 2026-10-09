fn main() {
    let path = std::env::args().nth(1).expect("path");
    let scene =
        russimp::scene::Scene::from_file(&path, vec![russimp::scene::PostProcess::Triangulate]);
    println!("{:?}", scene.map(|s| s.meshes.len()));
}
