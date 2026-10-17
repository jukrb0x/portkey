#ifndef PORTKEY_ROUTE_IDENTITY_H
#define PORTKEY_ROUTE_IDENTITY_H
#include <stdbool.h>
#include <string.h>
#include <stdio.h>
// Controller and endpoint names must identify the same numbered external engine.
static inline bool displayRouteNamesMatch(const char *display, const char *endpoint, const char *controller) {
    if (!display || !endpoint || !controller || strncmp(display,"dispext",7)!=0 || !display[7]) return false;
    for (const char *p=display+7;*p;p++) if (*p<'0'||*p>'9') return false;
    char expected[128], prefix[128];
    if (snprintf(expected,sizeof(expected),"dcpext%s",display+7)>=(int)sizeof(expected) ||
        snprintf(prefix,sizeof(prefix),"%s:dcpav-service-epic:",display)>=(int)sizeof(prefix)) return false;
    if (strcmp(controller,expected)!=0 || strncmp(endpoint,prefix,strlen(prefix))!=0) return false;
    const char *suffix=endpoint+strlen(prefix); if (!*suffix) return false;
    for (;*suffix;suffix++) if (*suffix<'0'||*suffix>'9') return false;
    return true;
}
#endif
