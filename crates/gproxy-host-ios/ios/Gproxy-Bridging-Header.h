#include <stdlib.h>

// UTF-8 JSON result owned by Rust. Release exactly once with gproxy_ios_free.
char *gproxy_ios_call(const char *command, const char *input);
void gproxy_ios_free(char *value);
