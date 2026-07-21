#ifndef FUNCHOOK_COMPAT_H
#define FUNCHOOK_COMPAT_H 1

#include <stdarg.h>
#include <stddef.h>

void *funchook_memcpy(void *dst, const void *src, size_t len);
void *funchook_memmove(void *dst, const void *src, size_t len);
void *funchook_memset(void *dst, int value, size_t len);
int funchook_memcmp(const void *left, const void *right, size_t len);
size_t funchook_strlen(const char *str);
int funchook_strcmp(const char *left, const char *right);
char *funchook_strcpy(char *dst, const char *src);
char *funchook_strncpy(char *dst, const char *src, size_t len);
char *funchook_strcat(char *dst, const char *src);
int funchook_vsnprintf(char *dst, size_t size, const char *format, va_list args);
int funchook_snprintf(char *dst, size_t size, const char *format, ...);
int funchook_printf(const char *format, ...);

void *funchook_rust_alloc(size_t size);
void *funchook_rust_calloc(size_t count, size_t size);
void *funchook_rust_realloc(void *ptr, size_t size);
void funchook_rust_free(void *ptr);

#endif
