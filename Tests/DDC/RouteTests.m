@import Foundation;
@import IOKit;
#include "ioregistry.h"
#include <assert.h>
#include <stdio.h>

typedef struct { const char *name; unsigned parent; int references; bool external; bool bridge; } Node;
static Node nodes[100];
static unsigned candidates[8], candidateCount, cursor, openedProxy;
static bool validIterator, failOpen;
static kern_return_t fakeParent(io_registry_entry_t entry, const io_name_t plane, io_registry_entry_t *parent) {
    (void)plane;
    assert(entry < 100);
    if (!nodes[entry].parent) return KERN_FAILURE;
    *parent = nodes[entry].parent;
    nodes[*parent].references++;
    return KERN_SUCCESS;
}
static kern_return_t fakeName(io_registry_entry_t entry, io_name_t name) {
    assert(entry < 100);
    if (!nodes[entry].name) return KERN_FAILURE;
    snprintf(name, sizeof(io_name_t), "%s", nodes[entry].name);
    return KERN_SUCCESS;
}
static boolean_t fakeEqual(io_object_t first, io_object_t second) { return first == second; }
static kern_return_t fakeRelease(io_object_t object) {
    assert(object < 100 && nodes[object].references > 0);
    nodes[object].references--;
    return KERN_SUCCESS;
}
static CFTypeRef fakeProperty(io_registry_entry_t entry, CFStringRef key, CFAllocatorRef allocator, IOOptionBits options) {
    (void)allocator; (void)options;
    assert(entry < 100);
    if (CFEqual(key, CFSTR("Location"))) return CFRetain(nodes[entry].external ? CFSTR("External") : CFSTR("Embedded"));
    if (CFEqual(key, CFSTR("EPICProviderClass"))) return CFRetain(nodes[entry].bridge ? CFSTR("AppleDCPMCDP29XX") : CFSTR("AppleDCPPS190"));
    abort();
}
static CFMutableDictionaryRef fakeMatching(const char *name) {
    assert(strcmp(name, "DCPAVServiceProxy") == 0);
    return CFDictionaryCreateMutable(NULL, 0, NULL, NULL);
}
static kern_return_t fakeServices(mach_port_t port, CFDictionaryRef matching, io_iterator_t *iterator) {
    (void)port; CFRelease(matching);
    *iterator = 99; nodes[99].references++;
    return KERN_SUCCESS;
}
static io_object_t fakeNext(io_iterator_t iterator) {
    assert(iterator == 99);
    if (cursor == candidateCount) return MACH_PORT_NULL;
    unsigned entry = candidates[cursor++];
    nodes[entry].references++;
    return entry;
}
static boolean_t fakeValid(io_iterator_t iterator) { assert(iterator == 99); return validIterator; }
static IOAVServiceRef fakeOpen(CFAllocatorRef allocator, io_service_t service) {
    (void)allocator; openedProxy = service;
    if (failOpen) return NULL;
    return CFStringCreateWithFormat(NULL, NULL, CFSTR("mock-proxy-%u"), service);
}

// Compile the production selector itself against a small in-memory IOKit graph.
#define IORegistryEntryGetParentEntry fakeParent
#define IORegistryEntryGetName fakeName
#define IOObjectIsEqualTo fakeEqual
#define IOObjectRelease fakeRelease
#define IORegistryEntryCreateCFProperty fakeProperty
#define IOServiceMatching fakeMatching
#define IOServiceGetMatchingServices fakeServices
#define IOIteratorNext fakeNext
#define IOIteratorIsValid fakeValid
#define IOAVServiceCreateWithService fakeOpen
#include "../../ThirdParty/m1ddc/sources/ioregistry.m"

static void reset(void) {
    memset(nodes, 0, sizeof(nodes));
    memset(candidates, 0, sizeof(candidates));
    candidateCount = cursor = openedProxy = 0;
    validIterator = true; failOpen = false;
    nodes[1] = (Node){.name="IOMobileFramebufferShim", .parent=2};
    nodes[2] = (Node){.name="dispext0", .parent=3};
    nodes[3] = (Node){.name="AppleSoCIO", .parent=4};
    nodes[4] = (Node){.name="Root"};
    nodes[10] = (Node){.name="DCPAVServiceProxy", .parent=11, .external=true};
    nodes[11] = (Node){.name="dispext0:dcpav-service-epic:0", .parent=12};
    nodes[12] = (Node){.name="endpoint", .parent=13};
    nodes[13] = (Node){.name="dcpext0", .parent=3};
    // Identical names beneath a different registry parent must not match.
    nodes[20] = (Node){.name="DCPAVServiceProxy", .parent=21, .external=true};
    nodes[21] = (Node){.name="dispext0:dcpav-service-epic:0", .parent=22};
    nodes[22] = (Node){.name="endpoint", .parent=23};
    nodes[23] = (Node){.name="dcpext0", .parent=24};
    nodes[24] = (Node){.name="AppleSoCIO", .parent=4};
    // A second proxy on the selected route makes identity ambiguous.
    nodes[30] = (Node){.name="DCPAVServiceProxy", .parent=31, .external=true};
    nodes[31] = (Node){.name="dispext0:dcpav-service-epic:1", .parent=13};
}
static void check(bool expected, unsigned selected, unsigned chip) {
    DisplayInfos display = {}; display.adapter = 1;
    DDCTransport result = getDisplayDDCTransport(&display);
    assert((result.service != NULL) == expected);
    if (expected) {
        assert(openedProxy == selected);
        assert(result.chipAddress == chip);
        CFRelease(result.service);
    }
    for (unsigned i = 0; i < 100; i++) assert(nodes[i].references == 0);
}
int main(void) { @autoreleasepool {
    reset(); candidates[0]=20; candidateCount=1; check(false,0,0);
    reset(); candidates[0]=20; candidates[1]=10; candidateCount=2; check(true,10,0x37);
    reset(); candidates[0]=10; candidates[1]=20; candidateCount=2; check(true,10,0x37);
    reset(); candidates[0]=10; candidates[1]=30; candidateCount=2; check(false,0,0);
    reset(); candidates[0]=10; candidateCount=1; validIterator=false; check(false,0,0);
    reset(); candidates[0]=10; candidateCount=1; failOpen=true; check(false,0,0);
    reset(); candidates[0]=10; candidateCount=1; nodes[11].bridge=true; check(true,10,0xb7);
    reset(); candidates[0]=10; candidateCount=1; nodes[10].external=false; check(false,0,0);
    puts("DDC route selection: 8 simulated registry scenarios passed; references balanced");
} }
