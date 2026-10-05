//! Display history only. Never grants recovery, backup or firmware-write eligibility.
use crate::{battery::Levels, runtime_recovery::Role};
use nusb::MaybeFuture;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Read, Write},
    path::PathBuf,
    sync::Mutex,
};

#[derive(Clone, Debug, Deserialize, Serialize)]
struct Binding {
    serial: String,
}
#[derive(Default, Deserialize, Serialize)]
struct History {
    schema: u8,
    levels: Levels,
    recovery: [Option<Binding>; 3],
}
static LOCK: Mutex<()> = Mutex::new(());
fn path() -> Option<PathBuf> {
    Some(
        crate::host_storage::application_root()
            .ok()?
            .join("status.json"),
    )
}

fn load() -> History {
    let read = || -> Option<History> {
        let mut bytes = Vec::new();
        fs::File::open(path()?)
            .ok()?
            .take(8193)
            .read_to_end(&mut bytes)
            .ok()?;
        if bytes.len() > 8192 {
            return None;
        }
        let history: History = serde_json::from_slice(&bytes).ok()?;
        if history.schema != 1
            || [history.levels.left, history.levels.right]
                .into_iter()
                .flatten()
                .any(|n| n > 100)
            || history
                .recovery
                .iter()
                .flatten()
                .any(|b| b.serial.is_empty() || b.serial.len() > 128)
        {
            return None;
        }
        Some(history)
    };
    read().unwrap_or_default()
}
fn save(mut history: History) -> std::io::Result<()> {
    let Some(path) = path() else {
        return Ok(());
    };
    history.schema = 1;
    fs::create_dir_all(path.parent().expect("cache parent"))?;
    let temporary = path.with_extension("tmp");
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temporary)?;
    file.write_all(&serde_json::to_vec(&history)?)?;
    file.sync_all()?;
    fs::rename(temporary, path)
}
pub(crate) fn levels() -> Levels {
    load().levels
}
pub(crate) fn save_levels(levels: Levels) {
    let Ok(_guard) = LOCK.lock() else {
        return;
    };
    let mut history = load();
    history.levels = levels;
    let _ = save(history);
}
fn index(role: Role) -> usize {
    match role {
        Role::Left => 0,
        Role::Right => 1,
        Role::Receiver => 2,
    }
}

/// Called only after Session has correlated and validated the recovery drive.
pub(crate) fn confirm(role: Role, location: u64) {
    let Ok(devices) = nusb::list_devices().wait() else {
        return;
    };
    let mut matches = devices.filter(|d| {
        let same_port = crate::device::usb_location(d) == Some(location);
        same_port
            && d.vendor_id() == 0x239a
            && d.product_id() == 0x0029
            && d.product_string() == Some("NocFree &")
    });
    let Some(device) = matches.next() else {
        return;
    };
    if matches.next().is_some() {
        return;
    }
    let Some(serial) = device
        .serial_number()
        .filter(|s| !s.is_empty() && s.len() <= 128)
    else {
        return;
    };
    let Ok(_guard) = LOCK.lock() else {
        return;
    };
    let mut history = load();
    // A serial may label only one component.
    for binding in &mut history.recovery {
        if binding.as_ref().is_some_and(|b| b.serial == serial) {
            *binding = None;
        }
    }
    history.recovery[index(role)] = Some(Binding {
        serial: serial.into(),
    });
    let _ = save(history);
}
fn resolve(history: &History, live: &[(u64, String)]) -> [Option<u64>; 3] {
    std::array::from_fn(|role| {
        let binding = history.recovery[role].as_ref()?;
        if history
            .recovery
            .iter()
            .flatten()
            .filter(|b| b.serial == binding.serial)
            .count()
            != 1
        {
            return None;
        }
        let mut matching = live.iter().filter(|(_, serial)| *serial == binding.serial);
        let location = matching.next()?.0;
        if matching.next().is_some() {
            None
        } else {
            Some(location)
        }
    })
}
pub(crate) fn recovery_locations() -> [Option<u64>; 3] {
    let Ok(devices) = nusb::list_devices().wait() else {
        return [None; 3];
    };
    let live = devices
        .filter(|d| {
            d.vendor_id() == 0x239a
                && d.product_id() == 0x0029
                && d.product_string() == Some("NocFree &")
        })
        .filter_map(|d| {
            let location = crate::device::usb_location(&d)?;
            Some((location, d.serial_number()?.to_owned()))
        })
        .collect::<Vec<_>>();
    resolve(&load(), &live)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recovery_survives_restart_but_not_identity_replacement() {
        let mut history = History {
            schema: 1,
            ..History::default()
        };
        history.recovery[1] = Some(Binding {
            serial: "test-right".into(),
        });
        let restarted = serde_json::from_slice(&serde_json::to_vec(&history).unwrap()).unwrap();
        assert_eq!(
            resolve(&restarted, &[(7, "test-right".into())]),
            [None, Some(7), None]
        );
        assert_eq!(
            resolve(&restarted, &[(7, "another-board".into())]),
            [None; 3]
        );
        assert_eq!(resolve(&restarted, &[]), [None; 3]);
        assert_eq!(
            resolve(
                &restarted,
                &[(7, "test-right".into()), (8, "test-right".into())]
            ),
            [None; 3]
        );
        history.recovery[0] = history.recovery[1].clone();
        assert_eq!(resolve(&history, &[(7, "test-right".into())]), [None; 3]);
    }
}
