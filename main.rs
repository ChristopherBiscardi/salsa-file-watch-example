use std::path::{Path, PathBuf};

use notify::{watcher, DebouncedEvent, RecommendedWatcher, RecursiveMode, Watcher};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[salsa::query_group(VfsDatabaseStorage)]
trait VfsDatabase: salsa::Database + FileWatcher {
    fn read(&self, path: PathBuf) -> String;
}

trait FileWatcher {
    fn watch(&self, path: &Path);
    fn did_change_file(&mut self, path: &PathBuf);
}

fn read(db: &dyn VfsDatabase, path: PathBuf) -> String {
    db.salsa_runtime()
        .report_synthetic_read(salsa::Durability::LOW);

    db.watch(&path);
    std::fs::read_to_string(&path).unwrap_or_default()
}

#[salsa::database(VfsDatabaseStorage)]
struct MyDatabase {
    storage: salsa::Storage<Self>,
    watcher: Arc<Mutex<RecommendedWatcher>>,
}

impl<'a> salsa::Database for MyDatabase {}

impl FileWatcher for MyDatabase {
    fn watch(&self, path: &Path) {
        // Add a path to be watched. All files and directories at that path and
        // below will be monitored for changes.
        let mut watcher = self.watcher.lock().unwrap();
        watcher.watch(path, RecursiveMode::Recursive).unwrap();
    }
    fn did_change_file(&mut self, path: &PathBuf) {
        ReadQuery.in_db_mut(self).invalidate(&path.to_path_buf());
    }
}

fn main() {
    let (tx, rx) = channel();
    // Create a watcher object, delivering debounced events.
    // The notification back-end is selected based on the platform.
    let mut watcher = Arc::from(Mutex::new(watcher(tx, Duration::from_secs(1)).unwrap()));
    let mut db = MyDatabase {
        watcher,
        storage: salsa::Storage::default(),
    };
    let file_to_watch = Path::new("./test/something.txt");

    let contents = db.read(file_to_watch.to_path_buf());
    loop {
        match rx.recv() {
            Ok(event) => {
                println!("{:?}", event);
                match event {
                    DebouncedEvent::Write(filepath_buf) => {
                        db.did_change_file(&filepath_buf);
                        db.read(Path::new("./test/something2.txt").to_path_buf());
                    }
                    _ => {}
                }
            }
            Err(e) => println!("watch error: {:?}", e),
        }
    }
}
