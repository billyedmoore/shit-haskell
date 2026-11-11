/* HsBase.h: the C declarations base's foreign imports name, as hsc provides them (x86-64 Linux). */
#ifndef HSBASE_H
#define HSBASE_H

#ifndef _GNU_SOURCE
#define _GNU_SOURCE 1
#endif
#include "HsFFI.h"
#include "HsBaseConfig.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <errno.h>
#include <unistd.h>
#include <fcntl.h>
#include <signal.h>
#include <time.h>
#include <termios.h>
#include <utime.h>
#include <dirent.h>
#include <limits.h>
#include <locale.h>
#include <sys/types.h>
#include <sys/stat.h>
#include <sys/time.h>
#include <sys/times.h>
#include <sys/resource.h>
#include <sys/wait.h>
#include <sys/select.h>
#include <sys/utsname.h>
#include <sys/mman.h>
#include <sys/file.h>

/* The helpers of hsc's runtime (rts/hsc-rts-shim.c and the runtimes' own). */
HsInt __hscore_sizeof_termios(void);
HsInt __hscore_sizeof_sigset_t(void);
int __hscore_echo(void);
int __hscore_tcsanow(void);
int __hscore_icanon(void);
int __hscore_vmin(void);
int __hscore_vtime(void);
int __hscore_sigttou(void);
int __hscore_sig_block(void);
int __hscore_sig_setmask(void);
tcflag_t __hscore_lflag(struct termios *ts);
void __hscore_poke_lflag(struct termios *ts, tcflag_t t);
unsigned char *__hscore_ptr_c_cc(struct termios *ts);
void *__hscore_get_saved_termios(int fd);
void __hscore_set_saved_termios(int fd, void *ts);

#endif
