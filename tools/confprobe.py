#!/usr/bin/env python3
"""Fills in an autoconf config header template (`Hs*Config.h.in`) by probing the C toolchain, in place
of running the package's configure script: a development tool for vendoring the boot packages.

Usage: confprobe.py template.h.in output.h [PACKAGE_NAME VERSION]

Each `#undef NAME` of the template is decided by a small C test of our own:
  HAVE_X_Y_H                 the header x/y.h compiles
  HAVE_DECL_X                X is declared by the usual headers (1 or 0, as autoconf defines it)
  HAVE_STRUCT_S_F            struct s has the member f (HAVE_STRUCT_S alone: the struct exists)
  CONST_X                    the value of the constant X (-1 when it is not defined)
  HAVE_X (otherwise)         the function x links
plus a few macros configure computes specially.
"""
import os, re, subprocess, sys, tempfile

HEADERS = """
#define _GNU_SOURCE 1
#include <sys/types.h>
#include <sys/stat.h>
#include <sys/time.h>
#include <sys/times.h>
#include <sys/resource.h>
#include <sys/wait.h>
#include <sys/utsname.h>
#include <sys/mman.h>
#include <sys/sysmacros.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <strings.h>
#include <unistd.h>
#include <fcntl.h>
#include <dirent.h>
#include <signal.h>
#include <time.h>
#include <termios.h>
#include <pwd.h>
#include <grp.h>
#include <limits.h>
#include <spawn.h>
#include <semaphore.h>
#include <dlfcn.h>
#include <pty.h>
#include <utime.h>
"""

tmp = tempfile.mkdtemp(prefix="confprobe-")

def compiles(code, link=False, libs=()):
    c = os.path.join(tmp, "t.c")
    with open(c, "w") as fh:
        fh.write(code)
    cmd = ["gcc", "-w", c, "-o", os.path.join(tmp, "t")] + ([] if link else ["-c"]) + list(libs)
    return subprocess.run(cmd, capture_output=True).returncode == 0

def run_value(code):
    c = os.path.join(tmp, "v.c")
    with open(c, "w") as fh:
        fh.write(code)
    exe = os.path.join(tmp, "v")
    if subprocess.run(["gcc", "-w", c, "-o", exe], capture_output=True).returncode != 0:
        return None
    return subprocess.run([exe], capture_output=True, text=True).stdout.strip()

def header_name(rest):
    parts = rest.lower().split("_")
    if parts[0] in ("sys", "bsd", "minix", "arpa", "netinet", "net"):
        return parts[0] + "/" + "_".join(parts[1:]) + ".h"
    return "_".join(parts) + ".h"

def decide(name):
    if name.endswith("_H") and name.startswith("HAVE_"):
        h = header_name(name[5:-2])
        return "1" if compiles(f"#include <{h}>\nint x;\n") else None
    if name.startswith("HAVE_DECL_"):
        sym = name[len("HAVE_DECL_"):]
        for cand in (sym.lower(), sym):
            if compiles(HEADERS + f"int main(void) {{ (void)({cand}); return 0; }}\n"):
                return "1"
        return "0"
    if name.startswith("HAVE_STRUCT_"):
        rest = name[len("HAVE_STRUCT_"):].lower()
        for struct in ("stat", "dirent", "tm", "rlimit", "statx", "passwd", "timespec", "termios"):
            if rest == struct:
                return "1" if compiles(HEADERS + f"struct {struct} s;\n") else None
            if rest.startswith(struct + "_"):
                field = rest[len(struct) + 1:]
                return "1" if compiles(HEADERS + f"int main(void) {{ struct {struct} s; (void)&s.{field}; return 0; }}\n") else None
        return None
    if name.startswith("CONST_"):
        sym = name[len("CONST_"):]
        v = run_value(HEADERS + f"int main(void) {{\n#ifdef {sym}\nprintf(\"%ld\", (long)({sym}));\n#else\nprintf(\"-1\");\n#endif\nreturn 0; }}\n")
        return v if v is not None else "-1"
    special = {
        "HAVE_RTLDDEFAULT": HEADERS + "void *p = RTLD_DEFAULT;\n",
        "HAVE_RTLDNEXT": HEADERS + "void *p = RTLD_NEXT;\n",
        "HAVE_RTLDLOCAL": HEADERS + "int p = RTLD_LOCAL;\n",
        "HAVE_RTLDGLOBAL": HEADERS + "int p = RTLD_GLOBAL;\n",
        "HAVE_RTLDNOW": HEADERS + "int p = RTLD_NOW;\n",
        "HAVE_SC_GETGR_R_SIZE_MAX": HEADERS + "int p = _SC_GETGR_R_SIZE_MAX;\n",
        "HAVE_SC_GETPW_R_SIZE_MAX": HEADERS + "int p = _SC_GETPW_R_SIZE_MAX;\n",
        "HAVE_F_GETLK": HEADERS + "int p = F_GETLK;\n",
        "HAVE_CLOCKS_PER_SEC": HEADERS + "long p = CLOCKS_PER_SEC;\n",
        "HAVE_TM_ZONE": HEADERS + "int main(void) { struct tm t; (void)t.tm_zone; return 0; }\n",
        "HAVE_STATX_FUN": HEADERS + "int main(void) { struct statx s; return statx(0, \"\", 0, 0, &s); }\n",
        # As the package's sources see it: without _GNU_SOURCE before <sys/stat.h>.
        "HAVE_STATX_MNT_ID": "#include <sys/stat.h>\nint x = STATX_MNT_ID;\n",
    }
    if name in special:
        return "1" if compiles(special[name], link=name.endswith("_FUN")) else None
    if name == "HAVE_NO_PASSWD_PW_GECOS":
        return None if compiles(HEADERS + "int main(void) { struct passwd p; (void)p.pw_gecos; return 0; }\n") else "1"
    if name == "HAVE_TZNAME":
        return "1" if compiles(HEADERS + "int main(void) { return tzname[0] != 0; }\n", link=True) else None
    if name in ("HAVE_DEV_PTMX", "HAVE_DEV_PTC"):
        return "1" if os.path.exists("/dev/" + name[len("HAVE_DEV_"):].lower()) else None
    if name in ("STDC_HEADERS", "HAVE_WORKING_FORK", "HAVE_WORKING_VFORK"):
        return "1"
    if name == "EXE_EXTENSION":
        return '""'
    if name.startswith("HAVE_"):
        fn = name[5:].lower()
        code = f"char {fn}(void);\nint main(void) {{ return (int)(long)&{fn}; }}\n"
        return "1" if compiles(code, link=True, libs=("-lutil", "-lpthread", "-lrt", "-ldl")) else None
    return None

def main():
    src, out = sys.argv[1], sys.argv[2]
    pkg = sys.argv[3] if len(sys.argv) > 3 else ""
    ver = sys.argv[4] if len(sys.argv) > 4 else ""
    lines = []
    # AC_USE_SYSTEM_EXTENSIONS: the extension macros are defined (but not the ones for other systems).
    not_linux = {"_FILE_OFFSET_BITS", "_LARGE_FILES", "_MINIX", "_POSIX_SOURCE", "_POSIX_1_SOURCE", "_XOPEN_SOURCE"}
    for line in open(src):
        m = re.match(r"(\s*)# undef (\w+)", line)
        if m and m.group(2) not in not_linux and m.group(2).startswith("_"):
            lines.append(f"{m.group(1)}# define {m.group(2)} 1\n")
            continue
        m = re.match(r"#undef (\w+)", line)
        if not m:
            lines.append(line)
            continue
        name = m.group(1)
        if name.startswith("HAVE_BSD_"):
            # libbsd's headers would need -lbsd; GHC's own build does without them.
            lines.append(f"/* #undef {name} */\n")
            continue
        if name.startswith("PACKAGE_"):
            vals = {"PACKAGE_NAME": pkg, "PACKAGE_TARNAME": pkg, "PACKAGE_VERSION": ver,
                    "PACKAGE_STRING": f"{pkg} {ver}".strip(), "PACKAGE_BUGREPORT": "", "PACKAGE_URL": ""}
            lines.append(f'#define {name} "{vals.get(name, "")}"\n')
            continue
        v = decide(name)
        lines.append(f"#define {name} {v}\n" if v is not None else f"/* #undef {name} */\n")
    with open(out, "w") as fh:
        fh.write("/* Generated by tools/confprobe.py (hsc) for x86_64 Linux. */\n" + "".join(lines))

if __name__ == "__main__":
    main()
