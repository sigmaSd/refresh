use std::{
    ffi::CString,
    fs,
    io::{stdout, Write},
    sync::mpsc,
    thread::{self, sleep},
    time::{Duration, Instant},
};

mod libc {
    extern "C" {
        pub fn swapoff(path: *const i8) -> i32;
        pub fn swapon(path: *const i8, swapflags: i32) -> i32;
    }
}
type Result = std::result::Result<(), &'static str>;

fn main() -> Result {
    let (tx, rx) = mpsc::channel();
    swapoff(tx)?;
    animation(rx)?;
    swapon()?;
    Ok(())
}

fn animation(rx: mpsc::Receiver<Result>) -> Result {
    fn bar() -> &'static str {
        static mut C: usize = 0;
        unsafe {
            C += 1;

            match C % 3 {
                0 => "-",
                1 => "\\",
                2 => "|",
                3 => "/",
                _ => unreachable!(),
            }
        }
    }
    fn swap_remaining() -> String {
        let swaps = fs::read_to_string("/proc/swaps").unwrap();
        let used = (|| swaps.lines().nth(1)?.split_whitespace().nth(3))();
        used.map(|used| used.parse::<usize>().unwrap() / 1000)
            .map(|used| used.to_string())
            .unwrap_or_else(|| "0".to_owned())
    }
    let skip_first = Instant::now();
    loop {
        match rx.try_recv() {
            Ok(result) => {
                println!();
                return result;
            }
            Err(_) => {
                if skip_first.elapsed() < Duration::from_millis(200) {
                    continue;
                }

                print!("\r[{}] Remaining: {}Mb", bar(), swap_remaining());
                stdout().flush().unwrap();
                sleep(Duration::from_millis(100));
            }
        }
    }
}

fn swapoff(tx: mpsc::Sender<Result>) -> Result {
    fn find_active_swap() -> Option<String> {
        let swaps = fs::read_to_string("/proc/swaps").unwrap();
        swaps
            .lines()
            .nth(1)?
            .split_whitespace()
            .next()
            .map(ToOwned::to_owned)
    }

    thread::spawn(move || {
        let result = (|| {
            let swap_path = validate_swap_path(find_active_swap())?;
            runc(&|| unsafe { libc::swapoff(swap_path) })
        })();
        tx.send(result).unwrap();
    });
    Ok(())
}

fn swapon() -> Result {
    fn find_swap_path() -> Option<String> {
        let fstab = fs::read_to_string("/etc/fstab").unwrap();
        fstab
            .lines()
            .find(|line| line.contains("swap"))?
            .split_whitespace()
            .next()
            .map(ToOwned::to_owned)
    }
    let swap_path = validate_swap_path(find_swap_path())?;

    runc(&|| unsafe { libc::swapon(swap_path, 0) })
}

fn validate_swap_path(path: Option<String>) -> std::result::Result<*mut i8, &'static str> {
    let swap_path = if let Some(path) = path {
        path
    } else {
        return Err("No active swap found");
    };
    let swap_path = CString::new(swap_path).unwrap().into_raw();
    Ok(swap_path)
}

fn runc(f: &dyn Fn() -> i32) -> Result {
    if f() == -1 {
        return Err("Not superuser.");
    }
    Ok(())
}
