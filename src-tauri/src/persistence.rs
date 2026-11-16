use super::{unpoisoned, Shared, SAVING};
use quotis_core::Settings;
use std::path::Path;

/// The lock also covers publication, so a slow earlier save cannot announce last.
pub fn save<R>(
    shared: &Shared,
    dir: &Path,
    mut settings: Settings,
    write: impl FnOnce(&Settings, &Path) -> std::io::Result<()>,
    publish: impl FnOnce(&Settings) -> R,
) -> std::io::Result<R> {
    let _serial = unpoisoned(&SAVING);
    settings.sanitize();
    settings.position = unpoisoned(shared).engine.settings().position;
    write(&settings, dir)?;
    {
        let mut app = unpoisoned(shared);
        let position = app.engine.settings().position;
        if position != settings.position {
            settings.position = position;
            app.position_dirty = true;
        }
        app.engine.set_settings(settings.clone());
    }
    Ok(publish(&settings))
}

pub fn flush_position(shared: &Shared, dir: &Path) -> std::io::Result<()> {
    let _serial = unpoisoned(&SAVING);
    let settings = unpoisoned(shared).engine.settings().clone();
    settings.save(dir)?;
    let mut app = unpoisoned(shared);
    app.position_dirty = app.engine.settings().position != settings.position;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::App;
    use quotis_core::Engine;
    use std::sync::{mpsc, Arc, Mutex};
    use std::time::{Duration, Instant};

    fn fixture() -> (Shared, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "quotis-shell-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let app = App { engine: Engine::with_providers(Settings::default(), vec![]), size: (200.0, 32.0), placed_at: Instant::now(), position_dirty: false };
        (Arc::new(Mutex::new(app)), dir)
    }

    #[test]
    fn failed_save_keeps_runtime_and_never_publishes() {
        let (app, dir) = fixture();
        let settings = Settings { theme: "light".into(), ..Default::default() };
        let result = save(&app, &dir, settings, |_, _| Err(std::io::Error::other("disk failure")), |_| panic!("must not publish failed settings"));
        assert!(result.is_err());
        assert_eq!(unpoisoned(&app).engine.settings(), &Settings::default());
    }

    #[test]
    fn movement_during_save_survives_and_exit_flush_persists_it() {
        let (app, dir) = fixture();
        let position = Some((310, 120));
        let published = save(
            &app,
            &dir,
            Settings::default(),
            |settings, dir| {
                settings.save(dir)?;
                let mut app = unpoisoned(&app);
                app.engine.set_position(position);
                app.position_dirty = true;
                Ok(())
            },
            |settings| settings.clone(),
        )
        .unwrap();
        assert_eq!(published.position, position);
        assert!(unpoisoned(&app).position_dirty);
        flush_position(&app, &dir).unwrap();
        assert_eq!(Settings::load(&dir).position, position);
        assert!(!unpoisoned(&app).position_dirty);
    }

    #[test]
    fn failed_position_flush_retains_retry_state() {
        let (app, dir) = fixture();
        unpoisoned(&app).position_dirty = true;
        std::fs::create_dir(dir.join("settings.json")).unwrap();
        assert!(flush_position(&app, &dir).is_err());
        assert!(unpoisoned(&app).position_dirty);
    }

    #[test]
    fn overlapping_saves_publish_in_disk_and_runtime_order() {
        let (app, dir) = fixture();
        let (entered_tx, entered_rx) = mpsc::channel();
        let (resume_tx, resume_rx) = mpsc::channel();
        let published = Arc::new(Mutex::new(Vec::new()));
        let first = {
            let (app, dir, published) = (app.clone(), dir.clone(), published.clone());
            std::thread::spawn(move || {
                save(&app, &dir, Settings { theme: "dark".into(), ..Default::default() }, Settings::save, |s| {
                    entered_tx.send(()).unwrap();
                    resume_rx.recv().unwrap();
                    published.lock().unwrap().push(s.theme.clone());
                })
                .unwrap()
            })
        };
        entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let second = {
            let (app, dir, published) = (app.clone(), dir.clone(), published.clone());
            std::thread::spawn(move || {
                save(&app, &dir, Settings { theme: "light".into(), ..Default::default() }, Settings::save, |s| published.lock().unwrap().push(s.theme.clone()))
                    .unwrap()
            })
        };
        assert!(published.lock().unwrap().is_empty());
        resume_tx.send(()).unwrap();
        first.join().unwrap();
        second.join().unwrap();
        assert_eq!(*published.lock().unwrap(), ["dark", "light"]);
        assert_eq!(Settings::load(&dir), *unpoisoned(&app).engine.settings());
    }
}
