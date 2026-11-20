//! Emuladores conhecidos e onde guardam saves.

/// Um emulador: executáveis que o identificam e padrões de save.
/// Padrões com `<base>` valem para a pasta onde o executável foi achado (instalação portátil).
pub struct EmulatorDef {
    pub id: &'static str,
    pub name: &'static str,
    pub exes: &'static [&'static str],
    pub patterns: &'static [&'static str],
}

pub const EMULATORS: &[EmulatorDef] = &[
    EmulatorDef {
        id: "retroarch",
        name: "RetroArch",
        exes: &["retroarch.exe"],
        patterns: &[
            "<winAppData>/RetroArch/saves",
            "<winAppData>/RetroArch/states",
            "<base>/saves",
            "<base>/states",
        ],
    },
    EmulatorDef {
        id: "pcsx2",
        name: "PCSX2",
        exes: &["pcsx2-qt.exe", "pcsx2.exe"],
        patterns: &["<winDocuments>/PCSX2/memcards", "<base>/memcards"],
    },
    EmulatorDef {
        id: "duckstation",
        name: "DuckStation",
        exes: &["duckstation-qt-x64-ReleaseLTCG.exe", "duckstation-qt.exe"],
        patterns: &["<winDocuments>/DuckStation/memcards", "<base>/memcards"],
    },
    EmulatorDef {
        id: "dolphin",
        name: "Dolphin",
        exes: &["Dolphin.exe"],
        patterns: &[
            "<winDocuments>/Dolphin Emulator/GC",
            "<winDocuments>/Dolphin Emulator/Wii/title",
            "<winAppData>/Dolphin Emulator/GC",
            "<winAppData>/Dolphin Emulator/Wii/title",
            "<base>/User/GC",
            "<base>/User/Wii/title",
        ],
    },
    EmulatorDef {
        id: "ppsspp",
        name: "PPSSPP",
        exes: &["PPSSPPWindows64.exe", "PPSSPPWindows.exe"],
        patterns: &[
            "<winDocuments>/PPSSPP/PSP/SAVEDATA",
            "<base>/memstick/PSP/SAVEDATA",
        ],
    },
    EmulatorDef {
        id: "rpcs3",
        name: "RPCS3",
        exes: &["rpcs3.exe"],
        patterns: &["<base>/dev_hdd0/home/00000001/savedata"],
    },
    EmulatorDef {
        id: "cemu",
        name: "Cemu",
        exes: &["Cemu.exe"],
        patterns: &["<winAppData>/Cemu/mlc01/usr/save", "<base>/mlc01/usr/save"],
    },
    EmulatorDef {
        id: "mgba",
        name: "mGBA",
        exes: &["mGBA.exe"],
        patterns: &["<base>/*.sav"],
    },
    EmulatorDef {
        id: "ryujinx",
        name: "Ryujinx",
        exes: &["Ryujinx.exe"],
        patterns: &[
            "<winAppData>/Ryujinx/bis/user/save",
            "<base>/portable/bis/user/save",
        ],
    },
    EmulatorDef {
        id: "switch-yuzu",
        name: "Switch (yuzu e derivados)",
        exes: &[
            "yuzu.exe",
            "suyu.exe",
            "sudachi.exe",
            "citron.exe",
            "eden.exe",
        ],
        patterns: &[
            "<winAppData>/yuzu/nand/user/save",
            "<winAppData>/suyu/nand/user/save",
            "<winAppData>/sudachi/nand/user/save",
            "<winAppData>/citron/nand/user/save",
            "<winAppData>/eden/nand/user/save",
            "<base>/user/nand/user/save",
        ],
    },
];
