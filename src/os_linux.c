#include "config.h"
#include <elf.h>
#include <limits.h>
#include <stddef.h>
#include <stdint.h>
#ifdef FUNCHOOK_USE_DLSYM
#include <dlfcn.h>
#include <link.h>
#endif
#include "compat.h"
#include "funchook_internal.h"

#define LINUX_PROT_READ  0x1
#define LINUX_PROT_WRITE 0x2
#define LINUX_PROT_EXEC  0x4
#define LINUX_MAP_PRIVATE   0x02
#define LINUX_MAP_ANONYMOUS 0x20
#define LINUX_AT_FDCWD (-100)
#define LINUX_AT_PAGESZ 6
#define LINUX_EACCES 13

#if defined(CPU_X86_64)
#define SYS_READ 0
#define SYS_OPEN 2
#define SYS_CLOSE 3
#define SYS_MMAP 9
#define SYS_MPROTECT 10
#define SYS_MUNMAP 11

static long linux_syscall6(long number, long a1, long a2, long a3,
                           long a4, long a5, long a6)
{
    long result;
    register long r10 __asm__("r10") = a4;
    register long r8 __asm__("r8") = a5;
    register long r9 __asm__("r9") = a6;
    __asm__ volatile("syscall"
                     : "=a"(result)
                     : "a"(number), "D"(a1), "S"(a2), "d"(a3),
                       "r"(r10), "r"(r8), "r"(r9)
                     : "rcx", "r11", "memory");
    return result;
}

#elif defined(CPU_ARM64)
#define SYS_OPENAT 56
#define SYS_CLOSE 57
#define SYS_READ 63
#define SYS_MUNMAP 215
#define SYS_MMAP 222
#define SYS_MPROTECT 226

static long linux_syscall6(long number, long a1, long a2, long a3,
                           long a4, long a5, long a6)
{
    register long x0 __asm__("x0") = a1;
    register long x1 __asm__("x1") = a2;
    register long x2 __asm__("x2") = a3;
    register long x3 __asm__("x3") = a4;
    register long x4 __asm__("x4") = a5;
    register long x5 __asm__("x5") = a6;
    register long x8 __asm__("x8") = number;
    __asm__ volatile("svc 0"
                     : "+r"(x0)
                     : "r"(x1), "r"(x2), "r"(x3), "r"(x4), "r"(x5), "r"(x8)
                     : "memory");
    return x0;
}

#elif defined(CPU_X86)
#define SYS_READ 3
#define SYS_OPEN 5
#define SYS_CLOSE 6
#define SYS_MMAP 90
#define SYS_MUNMAP 91
#define SYS_MPROTECT 125

static long linux_syscall6(long number, long a1, long a2, long a3,
                           long a4, long a5, long a6)
{
    long result;
    unsigned long mmap_args[6];
    if (number == SYS_MMAP) {
        mmap_args[0] = (unsigned long)a1;
        mmap_args[1] = (unsigned long)a2;
        mmap_args[2] = (unsigned long)a3;
        mmap_args[3] = (unsigned long)a4;
        mmap_args[4] = (unsigned long)a5;
        mmap_args[5] = (unsigned long)a6;
        a1 = (long)mmap_args;
    }
    __asm__ volatile("int $0x80"
                     : "=a"(result)
                     : "a"(number), "b"(a1), "c"(a2), "d"(a3)
                     : "memory", "cc");
    return result;
}
#else
#error unsupported Linux architecture
#endif

static int linux_failed(long result)
{
    return (unsigned long)result >= (unsigned long)-4095;
}

static long linux_open_readonly(const char *path)
{
#ifdef SYS_OPENAT
    return linux_syscall6(SYS_OPENAT, LINUX_AT_FDCWD, (long)path, 0, 0, 0, 0);
#else
    return linux_syscall6(SYS_OPEN, (long)path, 0, 0, 0, 0, 0);
#endif
}

static long linux_read(int fd, void *buffer, size_t size)
{
    return linux_syscall6(SYS_READ, fd, (long)buffer, (long)size, 0, 0, 0);
}

static long linux_close(int fd)
{
    return linux_syscall6(SYS_CLOSE, fd, 0, 0, 0, 0, 0);
}

static void *linux_mmap(void *addr, size_t size, int prot)
{
    long result = linux_syscall6(SYS_MMAP, (long)addr, (long)size, prot,
                                 LINUX_MAP_PRIVATE | LINUX_MAP_ANONYMOUS,
                                 -1, 0);
    return linux_failed(result) ? (void *)-1 : (void *)result;
}

static long linux_mprotect(void *addr, size_t size, int prot)
{
    return linux_syscall6(SYS_MPROTECT, (long)addr, (long)size, prot, 0, 0, 0);
}

static long linux_munmap(void *addr, size_t size)
{
    return linux_syscall6(SYS_MUNMAP, (long)addr, (long)size, 0, 0, 0, 0);
}

size_t page_size;

static size_t linux_page_size(void)
{
    struct aux_entry {
        size_t tag;
        size_t value;
    } entries[32];
    long fd = linux_open_readonly("/proc/self/auxv");
    if (linux_failed(fd)) {
        return 0;
    }
    for (;;) {
        long count = linux_read((int)fd, entries, sizeof(entries));
        size_t i;
        if (count <= 0) {
            break;
        }
        for (i = 0; i + sizeof(entries[0]) <= (size_t)count; i += sizeof(entries[0])) {
            struct aux_entry *entry = (struct aux_entry *)((unsigned char *)entries + i);
            if (entry->tag == LINUX_AT_PAGESZ) {
                linux_close((int)fd);
                return entry->value;
            }
            if (entry->tag == 0) {
                linux_close((int)fd);
                return 0;
            }
        }
    }
    linux_close((int)fd);
    return 0;
}

funchook_t *funchook_alloc(void)
{
    if (page_size == 0) {
        page_size = linux_page_size();
        if (page_size == 0) {
            return NULL;
        }
    }
    return (funchook_t *)funchook_rust_calloc(1, funchook_size);
}

int funchook_free(funchook_t *funchook)
{
    funchook_rust_free(funchook);
    return 0;
}

#if defined(CPU_64BIT)
typedef struct {
    int fd;
    size_t pos;
    size_t length;
    unsigned char buffer[4096];
} memory_map_t;

static int memory_map_open(funchook_t *funchook, memory_map_t *map)
{
    long fd = linux_open_readonly("/proc/self/maps");
    if (linux_failed(fd)) {
        funchook_set_error_message(funchook, "Failed to open /proc/self/maps (error %d)", (int)-fd);
        return FUNCHOOK_ERROR_INTERNAL_ERROR;
    }
    map->fd = (int)fd;
    map->pos = 0;
    map->length = 0;
    return 0;
}

static int memory_map_char(memory_map_t *map, unsigned char *value)
{
    if (map->pos == map->length) {
        long count = linux_read(map->fd, map->buffer, sizeof(map->buffer));
        if (count <= 0) {
            return -1;
        }
        map->pos = 0;
        map->length = (size_t)count;
    }
    *value = map->buffer[map->pos++];
    return 0;
}

static int memory_map_address(memory_map_t *map, size_t *address, unsigned char delimiter)
{
    size_t value = 0;
    int digits = 0;
    unsigned char ch;
    while (memory_map_char(map, &ch) == 0) {
        unsigned digit;
        if (ch >= '0' && ch <= '9') digit = ch - '0';
        else if (ch >= 'a' && ch <= 'f') digit = ch - 'a' + 10;
        else if (ch >= 'A' && ch <= 'F') digit = ch - 'A' + 10;
        else if (ch == delimiter && digits) {
            *address = value;
            return 0;
        } else {
            return -1;
        }
        if (value > (SIZE_MAX - digit) / 16) {
            return -1;
        }
        value = value * 16 + digit;
        digits = 1;
    }
    return -1;
}

static int memory_map_next(memory_map_t *map, size_t *start, size_t *end)
{
    unsigned char ch;
    if (memory_map_address(map, start, '-') != 0 ||
        memory_map_address(map, end, ' ') != 0) {
        return -1;
    }
    while (memory_map_char(map, &ch) == 0) {
        if (ch == '\n') {
            return 0;
        }
    }
    return 0;
}

static void memory_map_close(memory_map_t *map)
{
    linux_close(map->fd);
}

static int get_free_address(funchook_t *funchook, void *func_addr, void *addrs[2])
{
    memory_map_t map;
    size_t prev_end = 0;
    size_t start, end;
    int rv = memory_map_open(funchook, &map);
    if (rv != 0) {
        return rv;
    }
    addrs[0] = addrs[1] = NULL;
    while (memory_map_next(&map, &start, &end) == 0) {
        if (prev_end <= SIZE_MAX - page_size && prev_end + page_size <= start) {
            if (start < (size_t)func_addr) {
                size_t addr = start - page_size;
                if ((size_t)func_addr - addr < INT32_MAX) {
                    addrs[0] = (void *)addr;
                }
            }
            if ((size_t)func_addr < prev_end) {
                if (prev_end - (size_t)func_addr < INT32_MAX) {
                    addrs[1] = (void *)prev_end;
                }
                memory_map_close(&map);
                return 0;
            }
        }
        prev_end = end;
    }
    if ((size_t)func_addr < prev_end) {
        if (prev_end - (size_t)func_addr < INT32_MAX) {
            addrs[1] = (void *)prev_end;
        }
        memory_map_close(&map);
        return 0;
    }
    memory_map_close(&map);
    funchook_set_error_message(funchook, "Could not find a free region near %p", func_addr);
    return FUNCHOOK_ERROR_MEMORY_ALLOCATION;
}

#define SAFE_JUMP_DISTANCE(X, Y) ((size_t)(X) - (size_t)(Y) < (INT32_MAX - page_size))
#endif

int funchook_page_alloc(funchook_t *funchook, funchook_page_t **page_out,
                        uint8_t *func, ip_displacement_t *disp)
{
    (void)disp;
#if defined(CPU_64BIT)
    int loop_count;
    for (loop_count = 0; loop_count < 3; loop_count++) {
        void *addrs[2];
        int rv = get_free_address(funchook, func, addrs);
        int i;
        if (rv != 0) return rv;
        for (i = 1; i >= 0; i--) {
            void *page;
            if (addrs[i] == NULL) continue;
            page = linux_mmap(addrs[i], page_size, LINUX_PROT_READ | LINUX_PROT_WRITE);
            if (page == (void *)-1) {
                funchook_set_error_message(funchook, "mmap failed at %p", addrs[i]);
                return FUNCHOOK_ERROR_MEMORY_ALLOCATION;
            }
            if (SAFE_JUMP_DISTANCE(func, page) || SAFE_JUMP_DISTANCE(page, func)) {
                *page_out = page;
                return 0;
            }
            linux_munmap(page, page_size);
        }
    }
    funchook_set_error_message(funchook, "Failed to allocate memory in unused regions");
    return FUNCHOOK_ERROR_MEMORY_ALLOCATION;
#else
    *page_out = linux_mmap(NULL, page_size, LINUX_PROT_READ | LINUX_PROT_WRITE);
    if (*page_out != (void *)-1) return 0;
    funchook_set_error_message(funchook, "mmap failed");
    return FUNCHOOK_ERROR_MEMORY_ALLOCATION;
#endif
}

int funchook_page_free(funchook_t *funchook, funchook_page_t *page)
{
    long rv = linux_munmap(page, page_size);
    if (!linux_failed(rv)) return 0;
    funchook_set_error_message(funchook, "Failed to deallocate page %p (error %d)", page, (int)-rv);
    return FUNCHOOK_ERROR_MEMORY_FUNCTION;
}

int funchook_page_protect(funchook_t *funchook, funchook_page_t *page)
{
    long rv = linux_mprotect(page, page_size, LINUX_PROT_READ | LINUX_PROT_EXEC);
    if (!linux_failed(rv)) return 0;
    funchook_set_error_message(funchook, "Failed to protect page %p (error %d)", page, (int)-rv);
    return FUNCHOOK_ERROR_MEMORY_FUNCTION;
}

int funchook_page_unprotect(funchook_t *funchook, funchook_page_t *page)
{
    long rv = linux_mprotect(page, page_size, LINUX_PROT_READ | LINUX_PROT_WRITE);
    if (!linux_failed(rv)) return 0;
    funchook_set_error_message(funchook, "Failed to unprotect page %p (error %d)", page, (int)-rv);
    return FUNCHOOK_ERROR_MEMORY_FUNCTION;
}

int funchook_unprotect_begin(funchook_t *funchook, mem_state_t *state,
                            void *start, size_t len)
{
    static int prot = LINUX_PROT_READ | LINUX_PROT_WRITE | LINUX_PROT_EXEC;
    size_t address = ROUND_DOWN((size_t)start, page_size);
    long rv;
    state->addr = (void *)address;
    state->size = ROUND_UP(len + (size_t)start - address, page_size);
    rv = linux_mprotect(state->addr, state->size, prot);
    if (!linux_failed(rv)) return 0;
    if (rv == -LINUX_EACCES && (prot & LINUX_PROT_EXEC)) {
        rv = linux_mprotect(state->addr, state->size, LINUX_PROT_READ | LINUX_PROT_WRITE);
        if (!linux_failed(rv)) {
            prot = LINUX_PROT_READ | LINUX_PROT_WRITE;
            return 0;
        }
    }
    funchook_set_error_message(funchook, "Failed to unprotect memory %p (error %d)", state->addr, (int)-rv);
    return FUNCHOOK_ERROR_MEMORY_FUNCTION;
}

int funchook_unprotect_end(funchook_t *funchook, const mem_state_t *state)
{
    long rv = linux_mprotect(state->addr, state->size, LINUX_PROT_READ | LINUX_PROT_EXEC);
    if (!linux_failed(rv)) return 0;
    funchook_set_error_message(funchook, "Failed to protect memory %p (error %d)", state->addr, (int)-rv);
    return FUNCHOOK_ERROR_MEMORY_FUNCTION;
}

void *funchook_resolve_func(funchook_t *funchook, void *func)
{
#ifdef FUNCHOOK_USE_DLSYM
    struct link_map *selected = NULL, *map;
    const ElfW(Ehdr) *header;
    const ElfW(Dyn) *dynamic;
    const ElfW(Sym) *symbols = NULL, *symbols_end;
    const char *strings = NULL;
    size_t strings_size = 0;
    int i;
    extern struct r_debug _r_debug __attribute__((weak));
    extern void *dlsym(void *, const char *) __attribute__((weak));
    if (&_r_debug == NULL || dlsym == NULL) return func;
    for (map = _r_debug.r_map; map != NULL; map = map->l_next) {
        if ((void *)map->l_addr <= func &&
            (selected == NULL || selected->l_addr < map->l_addr)) {
            selected = map;
        }
    }
    if (selected == NULL) return func;
    if (selected->l_addr != 0) {
        header = (const ElfW(Ehdr) *)selected->l_addr;
        if (funchook_memcmp(header->e_ident, ELFMAG, SELFMAG) != 0 ||
            (header->e_type != ET_EXEC && header->e_type != ET_DYN)) return func;
    }
    dynamic = selected->l_ld;
    for (i = 0; dynamic[i].d_tag != DT_NULL; i++) {
        if (dynamic[i].d_tag == DT_SYMTAB) symbols = (const ElfW(Sym) *)dynamic[i].d_un.d_ptr;
        else if (dynamic[i].d_tag == DT_STRTAB) strings = (const char *)dynamic[i].d_un.d_ptr;
        else if (dynamic[i].d_tag == DT_STRSZ) strings_size = dynamic[i].d_un.d_val;
    }
    if (symbols == NULL || strings == NULL) return func;
    symbols_end = (const ElfW(Sym) *)strings;
    while (symbols < symbols_end) {
        if (symbols->st_name >= strings_size) break;
        if (ELF64_ST_TYPE(symbols->st_info) == STT_FUNC && symbols->st_size == 0 &&
            (void *)symbols->st_value == func) {
            void *resolved = dlsym((void *)0, strings + symbols->st_name);
            if (resolved == func) resolved = dlsym((void *)-1, strings + symbols->st_name);
            if (resolved != NULL) func = resolved;
            break;
        }
        symbols++;
    }
#else
    (void)funchook;
#endif
    return func;
}
