#include <unistd.h>
#include <stdlib.h>
#include <stdio.h>
#include <string.h>

typedef struct napi_env__* napi_env;
typedef struct napi_value__* napi_value;
typedef struct napi_callback_info__* napi_callback_info;
typedef int napi_status;
typedef napi_value (*napi_callback)(napi_env, napi_callback_info);

extern napi_status napi_get_cb_info(
    napi_env env,
    napi_callback_info cbinfo,
    size_t* argc,
    napi_value* argv,
    napi_value* this_arg,
    void** data);

extern napi_status napi_get_value_string_utf8(
    napi_env env,
    napi_value value,
    char* buf,
    size_t bufsize,
    size_t* result);

extern napi_status napi_create_function(
    napi_env env,
    const char* utf8name,
    size_t length,
    napi_callback cb,
    void* data,
    napi_value* result);

static napi_value ExecReplace(napi_env env, napi_callback_info info) {
    size_t argc = 2;
    napi_value argv[2];
    napi_get_cb_info(env, info, &argc, argv, NULL, NULL);

    char bin_path[4096] = {0};
    size_t len = 0;
    if (argc >= 1) {
        napi_get_value_string_utf8(env, argv[0], bin_path, sizeof(bin_path), &len);
    }

    char instance_arg[64] = "0";
    if (argc >= 2) {
        napi_get_value_string_utf8(env, argv[1], instance_arg, sizeof(instance_arg), &len);
    }

    fflush(stdout);
    fflush(stderr);
    char *const args[] = { bin_path, instance_arg, NULL };
    execv(bin_path, args);
    perror("execv failed");
    return NULL;
}

__attribute__((visibility("default")))
napi_value napi_register_module_v1(napi_env env, napi_value exports) {
    napi_value fn = NULL;
    napi_create_function(env, "execReplace", 11, ExecReplace, NULL, &fn);
    return fn;
}
