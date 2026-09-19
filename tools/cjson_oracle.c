/* Oracle driver: parse a file with the original C cJSON and dump results. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "cJSON.h"

static char *read_file(const char *path, size_t *out_len)
{
    FILE *f = fopen(path, "rb");
    long length;
    char *buf;
    size_t n;
    if (f == NULL) {
        return NULL;
    }
    if (fseek(f, 0, SEEK_END) != 0) {
        fclose(f);
        return NULL;
    }
    length = ftell(f);
    if (length < 0) {
        fclose(f);
        return NULL;
    }
    rewind(f);
    buf = (char *)malloc((size_t)length + 1);
    if (buf == NULL) {
        fclose(f);
        return NULL;
    }
    n = fread(buf, 1, (size_t)length, f);
    fclose(f);
    buf[n] = '\0';
    if (out_len) {
        *out_len = n;
    }
    return buf;
}

int main(int argc, char **argv)
{
    char *buf;
    size_t len = 0;
    cJSON *tree;
    char *unformatted;
    char *formatted;
    const char *err;

    if (argc < 2) {
        fprintf(stderr, "usage: cjson_oracle <file>\n");
        return 2;
    }

    buf = read_file(argv[1], &len);
    if (buf == NULL) {
        fprintf(stderr, "cannot read %s\n", argv[1]);
        return 2;
    }

    tree = cJSON_Parse(buf);
    if (tree == NULL) {
        err = cJSON_GetErrorPtr();
        printf("FAIL\n");
        if (err != NULL && buf != NULL) {
            printf("POS %ld\n", (long)(err - buf));
        } else {
            printf("POS 0\n");
        }
        free(buf);
        return 0;
    }

    unformatted = cJSON_PrintUnformatted(tree);
    formatted = cJSON_Print(tree);
    if (unformatted == NULL || formatted == NULL) {
        printf("PRINT_FAIL\n");
        cJSON_Delete(tree);
        free(buf);
        return 0;
    }

    printf("OK\n");
    printf("U %zu\n", strlen(unformatted));
    fwrite(unformatted, 1, strlen(unformatted), stdout);
    printf("\n");
    printf("F %zu\n", strlen(formatted));
    fwrite(formatted, 1, strlen(formatted), stdout);
    printf("\n");

    cJSON_free(unformatted);
    cJSON_free(formatted);
    cJSON_Delete(tree);
    free(buf);
    return 0;
}
