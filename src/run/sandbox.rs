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
    collections::HashMap,
    ffi::OsStr,
    io,
    os::unix::process::ExitStatusExt,
    path::{Path, PathBuf},
    process::{Command, ExitStatus},
    ptr::addr_of_mut,
    sync::LazyLock,
};

pub static DEFAULT_SANDBOX_PATH: LazyLock<&Path> =
    LazyLock::new(|| Path::new("/tmp/flint-runtime/"));

pub struct Sandbox {
    mounts: Vec<Mount>,
    cwd: Option<PathBuf>,
}

pub struct Mount {
    pub host_path: PathBuf,
    pub sandbox_path: PathBuf,
}

pub enum ExitReason {
    Code(i32),
    Signal(i32),
}

impl ExitReason {
    #[must_use]
    pub fn success(&self) -> bool {
        matches!(self, ExitReason::Code(0))
    }
}

impl From<ExitStatus> for ExitReason {
    fn from(value: ExitStatus) -> Self {
        if let Some(code) = value.code() {
            return Self::Code(code);
        }

        // Processes can either exit with a code, or a signal.
        Self::Signal(value.signal().unwrap())
    }
}

impl Sandbox {
    pub fn create() -> io::Result<Self> {
        Ok(Self {
            mounts: Vec::new(),
            cwd: None,
        })
    }

    pub fn add_mount(&mut self, mount: Mount) {
        self.mounts.push(mount);
    }

    pub fn set_current_dir(&mut self, path: PathBuf) {
        self.cwd = Some(path);
    }

    pub fn run_sandboxed<S: AsRef<OsStr>, K: AsRef<OsStr> + Clone, V: AsRef<OsStr> + Clone>(
        self,
        exec: impl AsRef<OsStr>,
        args: &[S],
        envs: &HashMap<K, V>,
    ) -> io::Result<ExitReason> {
        // 4 MB
        let mut stack = vec![0u8; 4 * 1024 * 1024].into_boxed_slice();
        let flags = CloneFlags::CLONE_NEWNS
            | CloneFlags::CLONE_NEWIPC
            | CloneFlags::CLONE_NEWNET
            | CloneFlags::CLONE_NEWPID
            | CloneFlags::CLONE_NEWUTS
            | CloneFlags::CLONE_NEWUSER;
        let entrypoint = Box::new(|| {
            self.setup_child();
            let exit = self.run_unsandboxed(&exec, args, envs.clone()).unwrap();
            let code = match exit {
                ExitReason::Code(code) => code,
                ExitReason::Signal(sig) => 128 + sig,
            };
            code as isize
        });

        let pid =
            unsafe { clone(entrypoint, &mut *addr_of_mut!(stack), flags, Some(SIGCHLD)) }.unwrap();
        let status = waitpid(pid, None).unwrap();

        match status {
            WaitStatus::Exited(_, code) => Ok(ExitReason::Code(code)),
            WaitStatus::Signaled(_, sig, _) => Ok(ExitReason::Signal(sig as i32)),
            _ => unimplemented!(), // These are unrepresentable and implausible.
        }
    }

    pub fn run_unsandboxed<
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
        E: IntoIterator<Item = (K, V)>,
        K: AsRef<OsStr>,
        V: AsRef<OsStr>,
    >(
        &self,
        exec: impl AsRef<OsStr>,
        args: I,
        envs: E,
    ) -> io::Result<ExitReason> {
        if let Some(cwd) = &self.cwd {
            std::env::set_current_dir(cwd).expect("could not set current directory in sandbox");
        }

        Ok(std::process::Command::new(exec)
            .args(args)
            .envs(envs)
            .status()?
            .into())
    }

    fn setup_child(&self) {
        // Kill on parents death
        set_pdeathsig(Signal::SIGKILL).expect("Could not set pdeathsig");

        // Setup Loopback
        Self::setup_lo().expect("Loopback error");

        for mount_request in &self.mounts {
            std::fs::create_dir_all(&mount_request.sandbox_path)
                .expect("could not create sandbox path for mount");

            // Mount every requested mount
            mount(
                Some(&mount_request.host_path),
                &mount_request.sandbox_path,
                None::<&str>,
                MsFlags::MS_BIND | MsFlags::MS_REC | MsFlags::MS_SLAVE,
                None::<&str>,
            )
            .expect("could not setup requested mount");
        }
    }

    fn setup_lo() -> io::Result<()> {
        Command::new("/usr/sbin/ip")
            .args(["link", "set", "lo", "up"])
            .output()?;
        Ok(())
    }
}
