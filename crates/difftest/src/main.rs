//! difftest: differential tester that runs a reference Haskell compiler (GHC)
//! and the compiler under test on the same corpus and compares the results.
//!
//! Corpus layout: any tree of `.hs` files. A file's category is the name of its
//! parent directory. Categories starting with `fail` must be rejected by the
//! oracle; every other category must be accepted and its program run.
//!
//! Optional sidecars next to `NAME.hs`:
//!   NAME.flags   extra compiler flags, whitespace separated
//!   NAME.stdin   bytes fed to the compiled program's standard input
//!
//! Comparison: accept/reject must agree; for run tests the exit code, stdout and
//! stderr of the program must agree. With `--diag`, compiler diagnostics must also
//! agree byte for byte.

use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const USAGE: &str = "usage: difftest [options] <corpus-dir>...
  --oracle <cmd>     reference compiler (default: ghc)
  --hsc <cmd>        compiler under test (default: target/release/hsc)
  -j, --jobs <n>     parallel workers (default: half the cores)
  --filter <substr>  only tests whose category/name contains <substr>
  --timeout <secs>   timeout for each compile and each run (default: 60)
  --diag             also require identical compiler diagnostics
  --no-cache         ignore cached oracle results
  -v, --verbose      show full output for failures
  --work <dir>       scratch directory (default: target/difftest)
  --bench <n>        time the programs instead: best of <n> runs of each binary,
                     the oracle's and hsc's (outputs must agree), one test at a time";

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0100_0000_01b3;

struct Opts {
    oracle: String,
    hsc: String,
    jobs: usize,
    filter: Option<String>,
    timeout: Duration,
    diag: bool,
    no_cache: bool,
    verbose: bool,
    work: PathBuf,
    roots: Vec<PathBuf>,
    /// Benchmark mode: the number of timed runs of each binary.
    bench: Option<usize>,
}

#[derive(Clone)]
struct Test {
    /// `category/stem`, unique across the corpus.
    id: String,
    category: String,
    stem: String,
    /// Directory containing the source; used as the compiler's working directory.
    dir: PathBuf,
    /// Source file name relative to `dir`.
    file: String,
    src: Vec<u8>,
    flags: Vec<String>,
    stdin: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq)]
struct Outcome {
    compile_ok: bool,
    compile_err: Vec<u8>,
    ran: Option<Ran>,
}

#[derive(Clone, Debug, PartialEq)]
struct Ran {
    exit: i32,
    timed_out: bool,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

enum Verdict {
    Pass,
    Fail(String),
    /// The corpus entry itself is inconsistent with the oracle.
    Broken(String),
}

struct Proc {
    code: i32,
    timed_out: bool,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut opts = match parse_args(args) {
        Ok(o) => o,
        Err(e) => {
            if !e.is_empty() {
                eprintln!("difftest: {e}");
            }
            eprintln!("{USAGE}");
            std::process::exit(2);
        }
    };

    if let Err(e) = fs::create_dir_all(&opts.work) {
        eprintln!("difftest: cannot create {}: {e}", opts.work.display());
        std::process::exit(2);
    }
    opts.work = fs::canonicalize(&opts.work).expect("work dir exists");
    opts.oracle = resolve_compiler(&opts.oracle);
    opts.hsc = resolve_compiler(&opts.hsc);

    let version = match oracle_version(&opts.oracle) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("difftest: oracle `{}` is not runnable: {e}", opts.oracle);
            std::process::exit(2);
        }
    };

    let tests = match discover(&opts.roots, &opts.filter) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("difftest: {e}");
            std::process::exit(2);
        }
    };
    if tests.is_empty() {
        eprintln!("difftest: no tests selected");
        std::process::exit(2);
    }

    if let Some(n) = opts.bench {
        let report = bench(&opts, &tests, n, &version);
        print!("{report}");
        let _ = fs::write(opts.work.join("bench.txt"), &report);
        return;
    }

    let total = tests.len();
    let jobs = opts.jobs.clamp(1, total);
    let opts = Arc::new(opts);
    let version = Arc::new(version);
    let queue = Arc::new(Mutex::new(tests));
    let (tx, rx) = mpsc::channel::<(String, Verdict)>();

    let mut handles = Vec::new();
    for _ in 0..jobs {
        let (opts, version, queue, tx) = (opts.clone(), version.clone(), queue.clone(), tx.clone());
        handles.push(thread::spawn(move || loop {
            let next = queue.lock().expect("queue lock").pop();
            let Some(test) = next else { break };
            let verdict = check(&opts, &test, &version);
            if tx.send((test.id.clone(), verdict)).is_err() {
                break;
            }
        }));
    }
    drop(tx);
    let mut results: Vec<(String, Verdict)> = rx.iter().collect();
    for h in handles {
        let _ = h.join();
    }
    results.sort_by(|a, b| a.0.cmp(&b.0));

    let report = render_report(&results, &version);
    print!("{report}");
    let _ = fs::write(opts.work.join("report.txt"), &report);

    let failed = results.iter().any(|(_, v)| matches!(v, Verdict::Fail(_)));
    std::process::exit(if failed { 1 } else { 0 });
}

fn parse_args(args: Vec<String>) -> Result<Opts, String> {
    let mut o = Opts {
        oracle: "ghc".into(),
        hsc: "target/release/hsc".into(),
        jobs: thread::available_parallelism().map(|n| n.get()).unwrap_or(2).div_ceil(2),
        filter: None,
        timeout: Duration::from_secs(60),
        diag: false,
        no_cache: false,
        verbose: false,
        work: PathBuf::from("target/difftest"),
        roots: Vec::new(),
        bench: None,
    };
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--oracle" => o.oracle = next_val(&mut it, &a)?,
            "--hsc" => o.hsc = next_val(&mut it, &a)?,
            "-j" | "--jobs" => {
                o.jobs = next_val(&mut it, &a)?.parse().map_err(|_| format!("bad value for {a}"))?
            }
            "--filter" => o.filter = Some(next_val(&mut it, &a)?),
            "--timeout" => {
                let secs: u64 = next_val(&mut it, &a)?.parse().map_err(|_| format!("bad value for {a}"))?;
                o.timeout = Duration::from_secs(secs);
            }
            "--work" => o.work = PathBuf::from(next_val(&mut it, &a)?),
            "--bench" => {
                let n: usize = next_val(&mut it, &a)?.parse().map_err(|_| format!("bad value for {a}"))?;
                o.bench = Some(n.max(1));
            }
            "--diag" => o.diag = true,
            "--no-cache" => o.no_cache = true,
            "-v" | "--verbose" => o.verbose = true,
            "-h" | "--help" => return Err(String::new()),
            s if s.starts_with('-') => return Err(format!("unknown option {s}")),
            _ => o.roots.push(PathBuf::from(a)),
        }
    }
    if o.roots.is_empty() {
        return Err("no corpus directory given".into());
    }
    Ok(o)
}

fn next_val(it: &mut impl Iterator<Item = String>, flag: &str) -> Result<String, String> {
    it.next().ok_or_else(|| format!("{flag} needs a value"))
}

/// Paths are canonicalised so that they still resolve after the child changes
/// directory. Bare command names are left for PATH lookup.
fn resolve_compiler(cmd: &str) -> String {
    if cmd.contains('/') {
        match fs::canonicalize(cmd) {
            Ok(p) => p.display().to_string(),
            Err(_) => cmd.to_string(),
        }
    } else {
        cmd.to_string()
    }
}

fn oracle_version(oracle: &str) -> Result<String, String> {
    let out = Command::new(oracle).arg("--version").output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!("exit status {}", out.status));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn discover(roots: &[PathBuf], filter: &Option<String>) -> Result<Vec<Test>, String> {
    let mut tests = Vec::new();
    for root in roots {
        walk(root, &mut tests)?;
    }
    if let Some(f) = filter {
        tests.retain(|t| t.id.contains(f.as_str()));
    }
    tests.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(tests)
}

fn walk(dir: &Path, tests: &mut Vec<Test>) -> Result<(), String> {
    let rd = fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut entries: Vec<PathBuf> = rd.filter_map(|e| e.ok().map(|e| e.path())).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            let name = p.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            if name.starts_with('.') || name.starts_with('_') {
                continue;
            }
            walk(&p, tests)?;
        } else if p.extension().is_some_and(|e| e == "hs") {
            tests.push(load(&p)?);
        }
    }
    Ok(())
}

fn load(path: &Path) -> Result<Test, String> {
    let dir = path.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."));
    let dir = fs::canonicalize(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let file = path.file_name().expect("file name").to_string_lossy().into_owned();
    let stem = path.file_stem().expect("stem").to_string_lossy().into_owned();
    let category = dir.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let src = fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let flags = fs::read_to_string(dir.join(format!("{stem}.flags")))
        .map(|s| s.split_whitespace().map(String::from).collect())
        .unwrap_or_default();
    let stdin = fs::read(dir.join(format!("{stem}.stdin"))).unwrap_or_default();
    Ok(Test {
        id: format!("{category}/{stem}"),
        category,
        stem,
        dir,
        file,
        src,
        flags,
        stdin,
    })
}

fn is_reject_category(category: &str) -> bool {
    category.starts_with("fail")
}

fn check(opts: &Opts, t: &Test, version: &str) -> Verdict {
    let oracle = match oracle_outcome(opts, t, version) {
        Ok(o) => o,
        Err(e) => return Verdict::Fail(format!("oracle could not be run: {e}")),
    };
    let run = !is_reject_category(&t.category);
    let hsc = match execute(&opts.hsc, t, &opts.work.join("hsc"), opts.timeout, run) {
        Ok(o) => o,
        Err(e) => return Verdict::Fail(format!("compiler could not be run: {e}")),
    };
    judge(t, &oracle, &hsc, opts.diag)
}

fn judge(t: &Test, o: &Outcome, h: &Outcome, diag: bool) -> Verdict {
    let reject = is_reject_category(&t.category);
    if reject && o.compile_ok {
        return Verdict::Broken("corpus: expected rejection, but the oracle accepted it".into());
    }
    if !reject && !o.compile_ok {
        return Verdict::Broken(format!("corpus: oracle rejected a run test: {}", first_line(&o.compile_err)));
    }
    if o.compile_ok != h.compile_ok {
        return Verdict::Fail(if h.compile_ok {
            "accepted a program the oracle rejects".into()
        } else {
            format!("rejected a program the oracle accepts: {}", first_line(&h.compile_err))
        });
    }
    if !o.compile_ok {
        if diag && o.compile_err != h.compile_err {
            return Verdict::Fail(format!("diagnostics differ: {}", first_diff(&o.compile_err, &h.compile_err)));
        }
        return Verdict::Pass;
    }
    if diag && o.compile_err != h.compile_err {
        return Verdict::Fail(format!("compiler output differs: {}", first_diff(&o.compile_err, &h.compile_err)));
    }
    let (Some(or), Some(hr)) = (&o.ran, &h.ran) else {
        return Verdict::Fail("program was not run".into());
    };
    if or.timed_out != hr.timed_out {
        return Verdict::Fail(format!("timeout: oracle {} vs hsc {}", or.timed_out, hr.timed_out));
    }
    if or.exit != hr.exit {
        return Verdict::Fail(format!("exit code: oracle {} vs hsc {}", or.exit, hr.exit));
    }
    if or.stdout != hr.stdout {
        return Verdict::Fail(format!("stdout: {}", first_diff(&or.stdout, &hr.stdout)));
    }
    if or.stderr != hr.stderr {
        return Verdict::Fail(format!("stderr: {}", first_diff(&or.stderr, &hr.stderr)));
    }
    Verdict::Pass
}

fn first_line(bytes: &[u8]) -> String {
    let s = String::from_utf8_lossy(bytes);
    s.lines().next().unwrap_or("").to_string()
}

fn first_diff(a: &[u8], b: &[u8]) -> String {
    let sa = String::from_utf8_lossy(a);
    let sb = String::from_utf8_lossy(b);
    let la: Vec<&str> = sa.lines().collect();
    let lb: Vec<&str> = sb.lines().collect();
    for i in 0..la.len().max(lb.len()) {
        let x = la.get(i).copied();
        let y = lb.get(i).copied();
        if x != y {
            return format!(
                "line {}: oracle {:?} vs hsc {:?}",
                i + 1,
                x.unwrap_or("<eof>"),
                y.unwrap_or("<eof>")
            );
        }
    }
    "same lines, different bytes (trailing newline or encoding)".into()
}

fn oracle_outcome(opts: &Opts, t: &Test, version: &str) -> Result<Outcome, String> {
    let key = cache_key(t, version);
    let cache_dir = opts.work.join("cache");
    let cache_file = cache_dir.join(format!("{key}.txt"));
    if !opts.no_cache {
        if let Ok(bytes) = fs::read(&cache_file) {
            if let Some(o) = decode(&bytes) {
                return Ok(o);
            }
        }
    }
    let run = !is_reject_category(&t.category);
    let o = execute(&opts.oracle, t, &opts.work.join("oracle"), opts.timeout, run)?;
    let timed_out = o.ran.as_ref().is_some_and(|r| r.timed_out);
    if !timed_out {
        let _ = fs::create_dir_all(&cache_dir);
        let _ = fs::write(&cache_file, encode(&o));
    }
    Ok(o)
}

fn cache_key(t: &Test, version: &str) -> String {
    let flags = t.flags.join(" ");
    let parts: [&[u8]; 6] = [
        version.as_bytes(),
        t.category.as_bytes(),
        t.file.as_bytes(),
        t.src.as_slice(),
        flags.as_bytes(),
        t.stdin.as_slice(),
    ];
    let mut h = FNV_OFFSET;
    for p in parts {
        h = fnv1a(h, p);
        h = fnv1a(h, &[0xff]);
    }
    format!("{h:016x}")
}

fn fnv1a(mut h: u64, bytes: &[u8]) -> u64 {
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(FNV_PRIME);
    }
    h
}

/// Compile one test with `compiler`, and run the result when `run` is set.
fn execute(compiler: &str, t: &Test, work: &Path, timeout: Duration, run: bool) -> Result<Outcome, String> {
    let outdir = work.join(&t.category).join(&t.stem);
    let objdir = outdir.join("obj");
    let bin = outdir.join("prog");
    let _ = fs::remove_dir_all(&outdir);
    fs::create_dir_all(&objdir).map_err(|e| format!("{}: {e}", objdir.display()))?;

    let mut cmd = Command::new(compiler);
    cmd.current_dir(&t.dir)
        .arg("-v0")
        .arg("-outputdir")
        .arg(&objdir)
        .arg("-o")
        .arg(&bin)
        .args(&t.flags)
        .arg(&t.file);
    let c = run_proc(cmd, &[], timeout).map_err(|e| format!("{compiler}: {e}"))?;
    let compile_ok = c.code == 0 && !c.timed_out && bin.exists();
    let mut out = Outcome {
        compile_ok,
        compile_err: if c.timed_out { b"compiler timed out".to_vec() } else { c.stderr },
        ran: None,
    };
    if compile_ok && run {
        let mut rc = Command::new(&bin);
        rc.current_dir(&outdir);
        let r = run_proc(rc, &t.stdin, timeout).map_err(|e| format!("{}: {e}", bin.display()))?;
        out.ran = Some(Ran {
            exit: r.code,
            timed_out: r.timed_out,
            stdout: r.stdout,
            stderr: r.stderr,
        });
    }
    Ok(out)
}

fn run_proc(mut cmd: Command, stdin: &[u8], timeout: Duration) -> std::io::Result<Proc> {
    cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn()?;
    if let Some(mut pipe) = child.stdin.take() {
        let input = stdin.to_vec();
        thread::spawn(move || {
            let _ = pipe.write_all(&input);
        });
    }
    let mut out_pipe = child.stdout.take().expect("stdout is piped");
    let mut err_pipe = child.stderr.take().expect("stderr is piped");
    let out_t = thread::spawn(move || {
        let mut v = Vec::new();
        let _ = out_pipe.read_to_end(&mut v);
        v
    });
    let err_t = thread::spawn(move || {
        let mut v = Vec::new();
        let _ = err_pipe.read_to_end(&mut v);
        v
    });

    let start = Instant::now();
    let mut timed_out = false;
    let status = loop {
        if let Some(st) = child.try_wait()? {
            break st;
        }
        if start.elapsed() >= timeout {
            timed_out = true;
            let _ = child.kill();
            break child.wait()?;
        }
        thread::sleep(Duration::from_millis(1));
    };
    let stdout = out_t.join().unwrap_or_default();
    let stderr = err_t.join().unwrap_or_default();
    let code = status.code().unwrap_or_else(|| -status.signal().unwrap_or(0));
    Ok(Proc {
        code,
        timed_out,
        stdout,
        stderr,
    })
}

/// Cache record: a sequence of `name len\n<bytes>\n` fields.
fn encode(o: &Outcome) -> Vec<u8> {
    let mut out = Vec::new();
    put(&mut out, "compile_ok", if o.compile_ok { b"1" } else { b"0" });
    put(&mut out, "compile_err", &o.compile_err);
    match &o.ran {
        None => put(&mut out, "ran", b"0"),
        Some(r) => {
            put(&mut out, "ran", b"1");
            put(&mut out, "exit", r.exit.to_string().as_bytes());
            put(&mut out, "timed_out", if r.timed_out { b"1" } else { b"0" });
            put(&mut out, "stdout", &r.stdout);
            put(&mut out, "stderr", &r.stderr);
        }
    }
    out
}

fn put(out: &mut Vec<u8>, name: &str, value: &[u8]) {
    out.extend_from_slice(format!("{name} {}\n", value.len()).as_bytes());
    out.extend_from_slice(value);
    out.push(b'\n');
}

fn decode(data: &[u8]) -> Option<Outcome> {
    let mut fields: Vec<(String, Vec<u8>)> = Vec::new();
    let mut pos = 0;
    while pos < data.len() {
        let nl = pos + data[pos..].iter().position(|&b| b == b'\n')?;
        let header = std::str::from_utf8(&data[pos..nl]).ok()?;
        let (name, len) = header.rsplit_once(' ')?;
        let len: usize = len.parse().ok()?;
        let start = nl + 1;
        let end = start.checked_add(len)?;
        if end >= data.len() || data[end] != b'\n' {
            return None;
        }
        fields.push((name.to_string(), data[start..end].to_vec()));
        pos = end + 1;
    }
    let get = |name: &str| fields.iter().find(|(n, _)| n == name).map(|(_, v)| v.clone());
    let flag = |name: &str| get(name).map(|v| v == b"1");
    let compile_ok = flag("compile_ok")?;
    let compile_err = get("compile_err")?;
    let ran = if flag("ran")? {
        let exit = std::str::from_utf8(&get("exit")?).ok()?.parse().ok()?;
        Some(Ran {
            exit,
            timed_out: flag("timed_out")?,
            stdout: get("stdout")?,
            stderr: get("stderr")?,
        })
    } else {
        None
    };
    Some(Outcome {
        compile_ok,
        compile_err,
        ran,
    })
}

fn render_report(results: &[(String, Verdict)], version: &str) -> String {
    let mut per_cat: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut lines = Vec::new();
    let mut broken = Vec::new();
    let (mut pass, mut total) = (0usize, 0usize);
    for (id, v) in results {
        let cat = id.split('/').next().unwrap_or("").to_string();
        let entry = per_cat.entry(cat).or_insert((0, 0));
        entry.1 += 1;
        total += 1;
        match v {
            Verdict::Pass => {
                entry.0 += 1;
                pass += 1;
            }
            Verdict::Fail(why) => lines.push(format!("FAIL    {id}: {}", clip(why))),
            Verdict::Broken(why) => broken.push(format!("BROKEN  {id}: {}", clip(why))),
        }
    }
    let mut out = String::new();
    out.push_str(&format!("oracle: {version}\n"));
    out.push_str("category            pass/total\n");
    for (cat, (p, t)) in &per_cat {
        out.push_str(&format!("{cat:<19} {p}/{t}\n"));
    }
    for l in lines.iter().chain(broken.iter()) {
        out.push_str(l);
        out.push('\n');
    }
    out.push_str(&format!("TOTAL   {pass}/{total} passing, {} broken corpus entries\n", broken.len()));
    out
}

fn clip(s: &str) -> String {
    const MAX: usize = 300;
    if s.chars().count() <= MAX {
        s.to_string()
    } else {
        let head: String = s.chars().take(MAX).collect();
        format!("{head}...")
    }
}

/// Benchmark mode: each test is compiled by both compilers and each binary is run `n` times; the
/// best wall-clock time of each is reported with their ratio (hsc / oracle), and the geometric mean
/// of the ratios at the end. Tests whose outputs differ are reported and left out of the mean.
fn bench(opts: &Opts, tests: &[Test], n: usize, version: &str) -> String {
    let mut out = format!("oracle: {version}\n{:<32} {:>10} {:>10} {:>8}\n", "test", "oracle ms", "hsc ms", "ratio");
    let mut logs: Vec<f64> = Vec::new();
    for t in tests {
        if is_reject_category(&t.category) {
            continue;
        }
        let built = |compiler: &str, side: &str| -> Result<PathBuf, String> {
            let o = execute(compiler, t, &opts.work.join(format!("bench-{side}")), opts.timeout, false)?;
            if !o.compile_ok {
                return Err(format!("{side} did not compile it: {}", first_line(&o.compile_err)));
            }
            Ok(opts.work.join(format!("bench-{side}")).join(&t.category).join(&t.stem).join("prog"))
        };
        let timed = |bin: &Path| -> Result<(Duration, Proc), String> {
            let mut best: Option<(Duration, Proc)> = None;
            for _ in 0..n {
                let mut cmd = Command::new(bin);
                cmd.current_dir(bin.parent().unwrap_or(Path::new(".")));
                let start = Instant::now();
                let p = run_proc(cmd, &t.stdin, opts.timeout).map_err(|e| e.to_string())?;
                let d = start.elapsed();
                if p.timed_out {
                    return Err("timed out".into());
                }
                if best.as_ref().is_none_or(|(b, _)| d < *b) {
                    best = Some((d, p));
                }
            }
            best.ok_or_else(|| "not run".into())
        };
        let line = (|| -> Result<String, String> {
            let (ob, hb) = (built(&opts.oracle, "oracle")?, built(&opts.hsc, "hsc")?);
            let (od, op) = timed(&ob)?;
            let (hd, hp) = timed(&hb)?;
            if op.stdout != hp.stdout || op.code != hp.code {
                return Err(format!("outputs differ: {}", first_diff(&op.stdout, &hp.stdout)));
            }
            let (oms, hms) = (od.as_secs_f64() * 1000.0, hd.as_secs_f64() * 1000.0);
            let ratio = hms / oms.max(0.001);
            logs.push(ratio.ln());
            Ok(format!("{:<32} {:>10.1} {:>10.1} {:>7.1}x", t.id, oms, hms, ratio))
        })()
        .unwrap_or_else(|e| format!("{:<32} {}", t.id, clip(&e)));
        out.push_str(&line);
        out.push('\n');
    }
    if !logs.is_empty() {
        let mean = (logs.iter().sum::<f64>() / logs.len() as f64).exp();
        out.push_str(&format!("geometric mean of the ratios: {mean:.1}x over {} programs\n", logs.len()));
    }
    out
}
