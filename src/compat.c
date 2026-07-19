#include "config.h"
#include <limits.h>
#include <stdint.h>
#include "compat.h"

void *funchook_memcpy(void *dst, const void *src, size_t len)
{
    unsigned char *d = dst;
    const unsigned char *s = src;
    size_t i;
    for (i = 0; i < len; i++) {
        d[i] = s[i];
    }
    return dst;
}

void *funchook_memmove(void *dst, const void *src, size_t len)
{
    unsigned char *d = dst;
    const unsigned char *s = src;
    size_t i;
    if (d <= s || d >= s + len) {
        return funchook_memcpy(dst, src, len);
    }
    for (i = len; i != 0; i--) {
        d[i - 1] = s[i - 1];
    }
    return dst;
}

void *funchook_memset(void *dst, int value, size_t len)
{
    unsigned char *d = dst;
    size_t i;
    for (i = 0; i < len; i++) {
        d[i] = (unsigned char)value;
    }
    return dst;
}

int funchook_memcmp(const void *left, const void *right, size_t len)
{
    const unsigned char *a = left;
    const unsigned char *b = right;
    size_t i;
    for (i = 0; i < len; i++) {
        if (a[i] != b[i]) {
            return (int)a[i] - (int)b[i];
        }
    }
    return 0;
}

size_t funchook_strlen(const char *str)
{
    const char *end = str;
    while (*end != '\0') {
        end++;
    }
    return (size_t)(end - str);
}

int funchook_strcmp(const char *left, const char *right)
{
    while (*left != '\0' && *left == *right) {
        left++;
        right++;
    }
    return (int)(unsigned char)*left - (int)(unsigned char)*right;
}

char *funchook_strcpy(char *dst, const char *src)
{
    char *out = dst;
    do {
        *out++ = *src;
    } while (*src++ != '\0');
    return dst;
}

char *funchook_strncpy(char *dst, const char *src, size_t len)
{
    size_t i = 0;
    while (i < len && src[i] != '\0') {
        dst[i] = src[i];
        i++;
    }
    while (i < len) {
        dst[i++] = '\0';
    }
    return dst;
}

char *funchook_strcat(char *dst, const char *src)
{
    funchook_strcpy(dst + funchook_strlen(dst), src);
    return dst;
}

typedef struct {
    char *dst;
    size_t size;
    size_t length;
} format_output_t;

static void format_putc(format_output_t *out, char value)
{
    if (out->size != 0 && out->length < out->size - 1) {
        out->dst[out->length] = value;
    }
    out->length++;
}

static void format_repeat(format_output_t *out, char value, int count)
{
    while (count-- > 0) {
        format_putc(out, value);
    }
}

static void format_unsigned(format_output_t *out, uint64_t value, unsigned base,
                            int uppercase, int negative, int alternate,
                            int width, int precision, int left, int zero)
{
    char digits[32];
    const char *alphabet = uppercase ? "0123456789ABCDEF" : "0123456789abcdef";
    int count = 0;
    int prefix = negative ? 1 : 0;
    int hex_prefix = alternate && base == 16 && value != 0 ? 2 : 0;
    int zeros;
    int spaces;
    if (value == 0) {
        if (precision != 0) {
            digits[count++] = '0';
        }
    } else {
        while (value != 0) {
            digits[count++] = alphabet[value % base];
            value /= base;
        }
    }
    zeros = precision > count ? precision - count : 0;
    if (zero && !left && precision < 0) {
        int wanted = width - prefix - hex_prefix - count;
        if (wanted > zeros) {
            zeros = wanted;
        }
    }
    spaces = width - prefix - hex_prefix - zeros - count;
    if (!left) {
        format_repeat(out, ' ', spaces);
    }
    if (negative) {
        format_putc(out, '-');
    }
    if (hex_prefix) {
        format_putc(out, '0');
        format_putc(out, uppercase ? 'X' : 'x');
    }
    format_repeat(out, '0', zeros);
    while (count-- > 0) {
        format_putc(out, digits[count]);
    }
    if (left) {
        format_repeat(out, ' ', spaces);
    }
}

enum format_length {
    LENGTH_DEFAULT,
    LENGTH_CHAR,
    LENGTH_SHORT,
    LENGTH_LONG,
    LENGTH_LONG_LONG,
    LENGTH_SIZE,
};

int funchook_vsnprintf(char *dst, size_t size, const char *format, va_list args)
{
    format_output_t out = { dst, size, 0 };
    while (*format != '\0') {
        int left = 0, zero = 0, alternate = 0, width = 0, precision = -1;
        enum format_length length = LENGTH_DEFAULT;
        char spec;
        if (*format != '%') {
            format_putc(&out, *format++);
            continue;
        }
        format++;
        for (;;) {
            if (*format == '-') left = 1;
            else if (*format == '0') zero = 1;
            else if (*format == '#') alternate = 1;
            else if (*format == '+' || *format == ' ') { /* unsupported sign flags */ }
            else break;
            format++;
        }
        if (*format == '*') {
            width = va_arg(args, int);
            format++;
            if (width < 0) {
                left = 1;
                width = -width;
            }
        } else {
            while (*format >= '0' && *format <= '9') {
                width = width * 10 + (*format++ - '0');
            }
        }
        if (*format == '.') {
            format++;
            precision = 0;
            if (*format == '*') {
                precision = va_arg(args, int);
                format++;
            } else {
                while (*format >= '0' && *format <= '9') {
                    precision = precision * 10 + (*format++ - '0');
                }
            }
        }
        if (*format == 'h') {
            length = LENGTH_SHORT;
            if (*++format == 'h') {
                length = LENGTH_CHAR;
                format++;
            }
        } else if (*format == 'l') {
            length = LENGTH_LONG;
            if (*++format == 'l') {
                length = LENGTH_LONG_LONG;
                format++;
            }
        } else if (*format == 'z' || *format == 't' || *format == 'j') {
            length = LENGTH_SIZE;
            format++;
        }
        spec = *format != '\0' ? *format++ : '\0';
        if (spec == '%') {
            format_putc(&out, '%');
        } else if (spec == 'c') {
            char value = (char)va_arg(args, int);
            if (!left) format_repeat(&out, ' ', width - 1);
            format_putc(&out, value);
            if (left) format_repeat(&out, ' ', width - 1);
        } else if (spec == 's') {
            const char *str = va_arg(args, const char *);
            size_t len, rendered_len;
            if (str == NULL) str = "(null)";
            len = funchook_strlen(str);
            if (precision >= 0 && len > (size_t)precision) len = (size_t)precision;
            rendered_len = len;
            if (!left) format_repeat(&out, ' ', width - (int)len);
            while (len-- != 0) format_putc(&out, *str++);
            if (left) format_repeat(&out, ' ', width - (int)rendered_len);
        } else if (spec == 'p') {
            uint64_t value = (uintptr_t)va_arg(args, void *);
            format_unsigned(&out, value, 16, 0, 0, 1, width, precision, left, zero);
        } else if (spec == 'd' || spec == 'i') {
            int64_t value;
            uint64_t magnitude;
            if (length == LENGTH_LONG_LONG) value = va_arg(args, long long);
            else if (length == LENGTH_LONG) value = va_arg(args, long);
            else if (length == LENGTH_SIZE) value = (intptr_t)va_arg(args, intptr_t);
            else value = va_arg(args, int);
            magnitude = value < 0 ? (uint64_t)(-(value + 1)) + 1 : (uint64_t)value;
            format_unsigned(&out, magnitude, 10, 0, value < 0, 0, width, precision, left, zero);
        } else if (spec == 'u' || spec == 'x' || spec == 'X' || spec == 'o') {
            uint64_t value;
            unsigned base = spec == 'o' ? 8 : (spec == 'u' ? 10 : 16);
            if (length == LENGTH_LONG_LONG) value = va_arg(args, unsigned long long);
            else if (length == LENGTH_LONG) value = va_arg(args, unsigned long);
            else if (length == LENGTH_SIZE) value = va_arg(args, size_t);
            else value = va_arg(args, unsigned int);
            format_unsigned(&out, value, base, spec == 'X', 0, alternate,
                            width, precision, left, zero);
        } else if (spec != '\0') {
            format_putc(&out, '%');
            format_putc(&out, spec);
        }
    }
    if (size != 0) {
        size_t end = out.length < size ? out.length : size - 1;
        dst[end] = '\0';
    }
    return out.length > INT_MAX ? INT_MAX : (int)out.length;
}

int funchook_snprintf(char *dst, size_t size, const char *format, ...)
{
    int result;
    va_list args;
    va_start(args, format);
    result = funchook_vsnprintf(dst, size, format, args);
    va_end(args);
    return result;
}

int funchook_printf(const char *format, ...)
{
    (void)format;
    return 0;
}
