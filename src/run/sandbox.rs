use nix::{
    libc::SIGCHLD,
    mount::{MsFlags, mount},
    sched::{CloneFlags, clone},
    sys::{
        prctl::set_pdeathsig,
        signal::Signal,
        wait::{WaitStatus, waitpid},
    },
};
use std::{
    io,
    path::{Path, PathBuf},
    process::{Command, ExitStatus},
    ptr::addr_of_mut,
};
use temp_dir::TempDir;

pub struct Sandbox {
    mounts: Vec<Mount>,
    /// Should be a temporary directory somewhere.
    tmp_root: PathBuf,
    _tmp_root_owner: TempDir,
}

pub struct Mount {
    pub host_path: PathBuf,
    pub sandbox_path: PathBuf,
}

impl Sandbox {
    pub fn create() -> io::Result<Self> {
        let tmp_dir = TempDir::new()?;

        Ok(Self {
            mounts: Vec::new(),
            tmp_root: tmp_dir.path().to_path_buf(),
            _tmp_root_owner: tmp_dir,
        })
    }

    pub fn add_mount(&mut self, mount: Mount) {
        self.mounts.push(mount);
    }

    pub fn run_sandboxed(self, exec: &str) -> io::Result<i32> {
        // 4 MB
        let mut stack = vec![0u8; 4 * 1024 * 1024].into_boxed_slice();
        let flags = CloneFlags::CLONE_NEWNS
            | CloneFlags::CLONE_NEWIPC
            | CloneFlags::CLONE_NEWNET
            | CloneFlags::CLONE_NEWPID
            | CloneFlags::CLONE_NEWUTS;
        let entrypoint = Box::new(|| {
            self.setup_child();
            self.run_unsandboxed(exec).unwrap();
            0
        });

        let pid =
            unsafe { clone(entrypoint, &mut *addr_of_mut!(stack), flags, Some(SIGCHLD)) }.unwrap();
        let status = waitpid(pid, None).unwrap();

        match status {
            WaitStatus::Exited(_, code) => Ok(code),
            _ => todo!(),
        }
    }

    pub fn run_unsandboxed(&self, exec: &str) -> io::Result<ExitStatus> {
        std::process::Command::new(exec).status()
    }

    fn setup_child(&self) {
        // Kill on parents death
        set_pdeathsig(Signal::SIGKILL).expect("Could not set pdeathsig");

        // Setup Loopback
        Self::setup_lo().expect("Loopback error");

        // Create the new root
        Self::clone_root(&self.tmp_root).expect("Root clone creation error");
        for mount_request in &self.mounts {
            // Mount every requested mount
            mount(
                Some(&mount_request.host_path),
                &self.tmp_root.join(&mount_request.sandbox_path),
                None::<&str>,
                MsFlags::MS_BIND | MsFlags::MS_REC | MsFlags::MS_SLAVE,
                None::<&str>,
            )
            .expect("could not setup requested mount");
        }

        // Pivot Root
        Self::pivot_root().expect("Root setup error");
    }

    fn setup_lo() -> io::Result<()> {
        Command::new("/usr/sbin/ip")
            .args(["link", "set", "lo", "up"])
            .output()?;
        Ok(())
    }

    fn clone_root(new_root: &Path) -> io::Result<()> {
        for entry in std::fs::read_dir("/")? {
            let entry = entry?;
            mount(
                Some(entry.file_name().as_os_str()),
                &new_root.join(entry.file_name()),
                None::<&str>,
                MsFlags::MS_BIND | MsFlags::MS_REC | MsFlags::MS_SLAVE,
                None::<&str>,
            )?;
        }

        Ok(())
    }

    fn pivot_root() -> io::Result<()> {
        todo!()
    }
}
