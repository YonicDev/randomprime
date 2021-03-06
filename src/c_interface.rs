
use serde::{Serialize, Deserialize};

use crate::{
    patches,
    door_meta::{Weights}
};

use std::{
    cell::Cell,
    collections::HashMap,
    ffi::{CStr, CString},
    fs::{File, OpenOptions},
    panic,
    path::Path,
    os::raw::c_char,
};

#[derive(Deserialize)]
struct ConfigBanner
{
    game_name: Option<String>,
    developer: Option<String>,

    game_name_full: Option<String>,
    developer_full: Option<String>,
    description: Option<String>,
}

#[derive(Deserialize)]
struct PatchConfig {
    skip_frigate: bool,
    skip_crater: bool,
    fix_flaaghra_music: bool,
    trilogy_iso: Option<String>,
    varia_heat_protection: bool,
    stagger_suit_damage: bool,
    skip_hudmemos: bool,
    powerbomb_lockpick: bool,
    enable_one_way_doors: bool,
    patch_map: bool,
}

#[derive(Deserialize)]
struct Config {
    input_iso: String,
    output_iso: String,
    seed: u64,
    door_weights: Weights,
    patch_settings: PatchConfig,
    starting_pickups: u64,
    excluded_doors: [HashMap<String,Vec<bool>>;5],
}

/*
#[derive(Deserialize)]
struct Config
{
    input_iso: String,
    output_iso: String,
    layout_string: String,

    #[serde(default)]
    iso_format: patches::IsoFormat,

    #[serde(default)]
    skip_frigate: bool,
    #[serde(default)]
    skip_hudmenus: bool,
    #[serde(default)]
    nonvaria_heat_damage: bool,
    #[serde(default)]
    staggered_suit_damage: bool,
    #[serde(default)]
    obfuscate_items: bool,
    #[serde(default)]
    auto_enabled_elevators: bool,

    #[serde(default)]
    skip_impact_crater: bool,
    #[serde(default)]
    enable_vault_ledge_door: bool,
    #[serde(default)]
    artifact_hint_behavior: patches::ArtifactHintBehavior,

    #[serde(default)]
    trilogy_disc_path: Option<String>,

    #[serde(default)]
    keep_fmvs: bool,

    starting_items: Option<u64>,
    #[serde(default)]
    comment: String,
    #[serde(default)]
    main_menu_message: String,

    banner: Option<ConfigBanner>,
}*/

#[derive(Serialize)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
enum CbMessage<'a>
{
    Success,
    Error {
        msg: &'a str,
    },
    Warning {
        msg: &'a str,
    },
    Progress {
        percent: f64,
        msg: &'a str,
    },
}

impl<'a> CbMessage<'a>
{
    fn success_json() -> CString
    {
        CString::new(serde_json::to_string(&CbMessage::Success).unwrap()).unwrap()
    }

    fn error_json(msg: &str) -> CString
    {
        let msg = CbMessage::fix_msg(msg);
        let cbmsg = CbMessage::Error { msg };
        CString::new(serde_json::to_string(&cbmsg).unwrap()).unwrap()
    }

    fn progress_json(percent: f64, msg: &str) -> CString
    {
        let msg = CbMessage::fix_msg(msg);
        let cbmsg = CbMessage::Progress { percent, msg };
        CString::new(serde_json::to_string(&cbmsg).unwrap()).unwrap()
    }

    fn warning_json(msg: &str) -> CString
    {
        let msg = CbMessage::fix_msg(msg);
        let cbmsg = CbMessage::Warning { msg };
        CString::new(serde_json::to_string(&cbmsg).unwrap()).unwrap()
    }

    /// Remove all of the bytes after the first null byte
    fn fix_msg(msg: &str) -> &str
    {
        if let Some(pos) = msg.bytes().position(|i| i == b'\0') {
            &msg[..pos]
        } else {
            msg
        }
    }
}


struct ProgressNotifier
{
    total_size: usize,
    bytes_so_far: usize,
    cb_data: *const (),
    cb: extern fn(*const (), *const c_char)
}

impl ProgressNotifier
{
    fn new(cb_data: *const (), cb: extern fn(*const (), *const c_char))
        -> ProgressNotifier
    {
        ProgressNotifier {
            total_size: 0,
            bytes_so_far: 0,
            cb, cb_data
        }
    }
}

impl structs::ProgressNotifier for ProgressNotifier
{
    fn notify_total_bytes(&mut self, total_size: usize)
    {
        self.total_size = total_size
    }

    fn notify_writing_file(&mut self, file_name: &reader_writer::CStr, file_bytes: usize)
    {
        let percent = self.bytes_so_far as f64 / self.total_size as f64 * 100.;
        let msg = format!("Writing file {:?}", file_name);
        (self.cb)(self.cb_data, CbMessage::progress_json(percent, &msg).as_ptr());
        self.bytes_so_far += file_bytes;
    }

    fn notify_writing_header(&mut self)
    {
        let percent = self.bytes_so_far as f64 / self.total_size as f64 * 100.;
        (self.cb)(self.cb_data, CbMessage::progress_json(percent, "Writing ISO header").as_ptr());
    }

    fn notify_flushing_to_disk(&mut self)
    {
        (self.cb)(
            self.cb_data,
            CbMessage::progress_json(100., "Flushing written data to the disk").as_ptr(),
        );
    }
    fn notify_stacking_warning(&mut self)
    {
        (self.cb)(
            self.cb_data,
            CbMessage::warning_json("Item randomized game. Skipping item randomizer configuration").as_ptr(),
        );
    }
}

fn inner(config_json: *const c_char, cb_data: *const (), cb: extern fn(*const (), *const c_char))
    -> Result<(), String>
{
    let config_json = unsafe { CStr::from_ptr(config_json) }.to_str()
        .map_err(|e| format!("JSON parse failed: {}", e))?;

    let config: Config = serde_json::from_str(&config_json)
        .map_err(|e| format!("JSON parse failed: {}", e))?;

    let input_iso_file = File::open(config.input_iso.trim())
                .map_err(|e| format!("Failed to open {}: {}", config.input_iso, e))?;
    let input_iso = unsafe { memmap::Mmap::map(&input_iso_file) }
            .map_err(|e| format!("Failed to open {}: {}", config.input_iso,  e))?;

    let output_iso = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&config.output_iso)
        .map_err(|e| format!("Failed to open {}: {}", config.output_iso, e))?;

    let layout_string = String::from("NCiq7nTAtTnqPcap9VMQk_o8Qj6ZjbPiOdYDB5tgtwL_f01-UpYklNGnL-gTu5IeVW3IoUiflH5LqNXB3wVEER4");

    let (pickup_layout, elevator_layout, item_seed) = crate::parse_layout(&layout_string)?;
    let seed = config.seed;

    let flaahgra_music_files = if config.patch_settings.fix_flaaghra_music {
        if let Some(path) = &config.patch_settings.trilogy_iso {
            Some(crate::extract_flaahgra_music_files(&path)?)
        } else {
            None
        }
    } else {
        None
    };

    let config = config;

    let mut banner = Some(ConfigBanner {
        game_name: Some(String::from("Metroid Prime")),
        developer: Some(String::from("YonicStudios")),

        game_name_full: Some(String::from("Metroid Prime Door Randomized")),
        developer_full: Some(String::from("YonicStudios")),
        description: Some(String::from("Metroid Prime, but door colors have been randomized")),
    });

    let mpdr_version = "MPDR v0.3";
    let mut comment_message:String = "Generated with ".to_owned();
    comment_message.push_str(mpdr_version);

    let parsed_config = patches::ParsedConfig {
        input_iso, output_iso,
        is_item_randomized: None,
        pickup_layout, elevator_layout, seed,
        item_seed,door_weights:config.door_weights,
        excluded_doors:config.excluded_doors,
        patch_map:config.patch_settings.patch_map,

        layout_string,

        iso_format: patches::IsoFormat::Iso,
        skip_frigate: config.patch_settings.skip_frigate,
        skip_hudmenus: config.patch_settings.skip_hudmemos,
        nonvaria_heat_damage: config.patch_settings.varia_heat_protection,
        staggered_suit_damage: config.patch_settings.stagger_suit_damage,
        powerbomb_lockpick: config.patch_settings.powerbomb_lockpick,
        keep_fmvs: false,
        obfuscate_items: false,
        auto_enabled_elevators: false,
        quiet: false,

        skip_impact_crater: config.patch_settings.skip_crater,
        enable_vault_ledge_door: config.patch_settings.enable_one_way_doors,
        artifact_hint_behavior: patches::ArtifactHintBehavior::default(),

        flaahgra_music_files,

        starting_items: Some(config.starting_pickups),
        comment: comment_message,
        main_menu_message: String::from(mpdr_version),

        quickplay: false,

        bnr_game_name: banner.as_mut().and_then(|b| b.game_name.take()),
        bnr_developer: banner.as_mut().and_then(|b| b.developer.take()),

        bnr_game_name_full: banner.as_mut().and_then(|b| b.game_name_full.take()),
        bnr_developer_full: banner.as_mut().and_then(|b| b.developer_full.take()),
        bnr_description: banner.as_mut().and_then(|b| b.description.take()),

        pal_override: false,
    };

    let pn = ProgressNotifier::new(cb_data, cb);
    patches::patch_iso(parsed_config, pn)?;
    Ok(())
}

#[no_mangle]
pub extern fn randomprime_patch_iso(config_json: *const c_char , cb_data: *const (),
                                    cb: extern fn(*const (), *const c_char))
{
    thread_local! {
        static PANIC_DETAILS: Cell<Option<(String, u32)>> = Cell::new(None);
    }
    panic::set_hook(Box::new(|pinfo| {
        PANIC_DETAILS.with(|pd| {
            pd.set(pinfo.location().map(|l| (l.file().to_owned(), l.line())));
        });
    }));
    let r = panic::catch_unwind(|| inner(config_json, cb_data, cb))
        .map_err(|e| {
            let msg = if let Some(e) = e.downcast_ref::<&'static str>() {
                e.to_string()
            } else if let Some(e) = e.downcast_ref::<String>() {
                e.clone()
            } else {
                format!("{:?}", e)
            };

            if let Some(pd) = PANIC_DETAILS.with(|pd| pd.replace(None)) {
                let path = Path::new(&pd.0);
                let mut comp = path.components();
                let found = path.components()
                    .skip(1)
                    .zip(&mut comp)
                    .find(|(c, _)| c.as_os_str() == "randomprime")
                    .is_some();
                // If possible, include the section of the path starting with the directory named
                // "randomprime". If no such directoy exists, just use the file name.
                let shortened_path = if found {
                    comp.as_path().as_os_str()
                } else {
                    path.file_name().unwrap_or("".as_ref())
                };
                format!("{} at {}:{}", msg, shortened_path.to_string_lossy(), pd.1)
            } else {
                msg
            }
        })
        .and_then(|i| i);

    match r {
        Ok(()) => cb(cb_data, CbMessage::success_json().as_ptr()),
        Err(msg) => cb(cb_data, CbMessage::error_json(&msg).as_ptr()),
    };
}
