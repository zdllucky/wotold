fn main() {
    // [perm-usage] `Info.plist` рядом с этим файлом подхватывает сам
    // `tauri_build::build()`: в релизе ключи домешиваются в Info.plist бандла,
    // в debug — вшиваются в бинарь секцией `__TEXT,__info_plist` (у `tauri dev`
    // бандла нет вовсе). Своего `-sectcreate` здесь быть не должно: он положил
    // бы в ту же секцию второй plist-документ.
    println!("cargo:rerun-if-changed=Info.plist");
    tauri_build::build()
}
