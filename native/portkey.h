#ifndef PORTKEY_H
#define PORTKEY_H
#ifdef __cplusplus
extern "C" {
#endif
/* All calls must use the UI/main thread. The opaque controller owns runtime state.
 * Each returned UTF-8 JSON string must be released once with pk_string_free.
 * Caller-owned action strings are borrowed only for the duration of pk_action.
 * A handle must not be used after pk_destroy. See docs/plans/native-ui.md. */
void *pk_create(void);
char *pk_snapshot(void *controller);
char *pk_action(void *controller, const char *action_json);
void pk_string_free(char *json);
void pk_destroy(void *controller);
#ifdef __cplusplus
}
#endif
#endif
