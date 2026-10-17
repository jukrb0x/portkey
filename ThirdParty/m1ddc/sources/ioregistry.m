@import Foundation;
@import IOKit;
@import ApplicationServices;
@import CoreGraphics;

#include "ioregistry.h"
#include "utils.h"
#include "route_identity.h"

static CFTypeRef getCFStringRef(io_service_t service, char* key) {
    CFStringRef cfstring = CFStringCreateWithCString(kCFAllocatorDefault, key, kCFStringEncodingASCII);
    return IORegistryEntrySearchCFProperty(service, kIOServicePlane, cfstring, kCFAllocatorDefault, kIORegistryIterateRecursively);
}

static Boolean isMCDP29XXProxy(io_service_t proxy) {
    io_registry_entry_t parent = MACH_PORT_NULL;
    if (IORegistryEntryGetParentEntry(proxy, kIOServicePlane, &parent) != KERN_SUCCESS) {
        return false;
    }

    Boolean isMCDP29XX = false;
    CFTypeRef providerClass = IORegistryEntryCreateCFProperty(
        parent,
        CFSTR("EPICProviderClass"),
        kCFAllocatorDefault,
        0
    );
    if (providerClass != NULL && CFGetTypeID(providerClass) == CFStringGetTypeID()) {
        isMCDP29XX = CFStringCompare(providerClass, CFSTR("AppleDCPMCDP29XX"), 0) == kCFCompareEqualTo;
    }

    if (providerClass != NULL) {
        CFRelease(providerClass);
    }
    IOObjectRelease(parent);
    return isMCDP29XX;
}

CGDisplayCount getOnlineDisplayInfos(DisplayInfos* displayInfos) {
    // Getting online display list and count
    CGDisplayCount screenCount;
    CGDirectDisplayID screenList[MAX_DISPLAYS];
    CGGetOnlineDisplayList(MAX_DISPLAYS, screenList, &screenCount);

    int validDisplayCount = 0;
    // Fetching each display infos from IOKit
    for (int i = 0; i < (int)screenCount && validDisplayCount < MAX_DISPLAYS; i++) {
        DisplayInfos *currDisplay = displayInfos + validDisplayCount;
        currDisplay->id = screenList[i];

        // This is a private API, but it's a shortcut to get the system UUID
        CFDictionaryRef displayInfoDict = CoreDisplay_DisplayCreateInfoDictionary(currDisplay->id);
        if (displayInfoDict == NULL) {
            // Skip virtual displays that don't provide info dictionary
            continue;
        }

        currDisplay->serial = CGDisplaySerialNumber(currDisplay->id);
        currDisplay->model = CGDisplayModelNumber(currDisplay->id);
        currDisplay->vendor = CGDisplayVendorNumber(currDisplay->id);

        currDisplay->uuid = CFDictionaryGetValue(displayInfoDict, CFSTR("kCGDisplayUUID"));
        currDisplay->ioLocation = CFDictionaryGetValue(displayInfoDict, CFSTR("IODisplayLocation"));

        // Skip virtual displays (like Sidecar/AirPlay) that don't have required properties
        if (currDisplay->uuid == NULL || currDisplay->ioLocation == NULL) {
            continue;
        }

        // Retrieving IORegistry entry for display
        currDisplay->adapter = IORegistryEntryCopyFromPath(kIOMainPortDefault, (CFStringRef)currDisplay->ioLocation);
        if (currDisplay->adapter == MACH_PORT_NULL) {
            continue;
        }

        // If successful, we can retrieve the EDID UUID, and other display attributes
        currDisplay->edid = getCFStringRef(currDisplay->adapter, "EDID UUID");
        CFDictionaryRef displayAttrs = getCFStringRef(currDisplay->adapter, "DisplayAttributes");
        if (displayAttrs) {
            NSDictionary* displayAttrsNS = (NSDictionary*)displayAttrs;
            NSDictionary* productAttrs = [displayAttrsNS objectForKey:@"ProductAttributes"];
            if (productAttrs) {
                currDisplay->productName = [productAttrs objectForKey:@"ProductName"] ?: @"Unknown Display";
                currDisplay->manufacturer = [productAttrs objectForKey:@"ManufacturerID"];
                currDisplay->alphNumSerial = [productAttrs objectForKey:@"AlphanumericSerialNumber"];
            }
        }

        validDisplayCount++;
    }

    return validDisplayCount;
}

/* 
 *  Returns display identifier based on identification method
 *  Allowed methods are:
 *  - id    Display ID                          "<id>"
 *  - uuid  Display UUID                        "<uuid>"
 *  - edid  Display EDID UUID                   "<edid>"
 *  - seid  Display Alphnum SN + EDID UUID      "<an_serial>:<edid>"
 *  - basic Match basic identifiers             "<vendor>:<model>:<serial>"
 *  - ext   Match basic + extended identifiers  "<vendor>:<model>:<serial>:<manufacturer>:<an_serial>:<name>"
 *  - full  Match basic + extended + location   "<vendor>:<model>:<serial>:<manufacturer>:<an_serial>:<name>:<location>"
 */
NSString *getDisplayIdentifier(DisplayInfos *display, char *identificationMethod) {
    if (STR_EQ(identificationMethod, "id")) {
        return [NSString stringWithFormat:@"%u", display->id];
    } else if (STR_EQ(identificationMethod, "uuid")) {
        return display->uuid ?: @"";
    } else if (STR_EQ(identificationMethod, "edid")) {
        return display->edid ?: @"";
    } else if (STR_EQ(identificationMethod, "seid")) {
        return [NSString stringWithFormat:@"%@:%@",
            display->alphNumSerial ?: @"",
            display->edid ?: @""];
    } else if (STR_EQ(identificationMethod, "basic")) {
        return [NSString stringWithFormat:@"%u:%u:%u",
            display->vendor,
            display->model,
            display->serial];
    } else if (STR_EQ(identificationMethod, "ext")) {
        return [NSString stringWithFormat:@"%d:%d:%d:%@:%@:%@",
            display->vendor,
            display->model,
            display->serial,
            display->manufacturer ?: @"",
            display->alphNumSerial ?: @"",
            display->productName ?: @""];
    } else if (STR_EQ(identificationMethod, "full")) {
        return [NSString stringWithFormat:@"%d:%d:%d:%@:%@:%@:%@",
            display->vendor,
            display->model,
            display->serial,
            display->manufacturer ?: @"",
            display->alphNumSerial ?: @"",
            display->productName ?: @"",
            display->ioLocation ?: @""];
    }
    return NULL;
}

DisplayInfos* selectDisplay(DisplayInfos *displays, int connectedDisplays, char *displayIdentifier) {

    // Checking if display identifier is a display index from the "list" command
    char *stop;
    long displayNumber = strtol(displayIdentifier, &stop, 10);
    if (*stop == '\0') {
        return displayNumber <= connectedDisplays ? displays + (displayNumber - 1) : NULL;
    }

    // Checking if an identification method is specified, otherwise defaulting to UUID
    char *identificationMethod = "uuid";
    char *delimiter = strstr(displayIdentifier, "=");
    if (delimiter != NULL) {
        // Delimiter should not be at the beginning or end of the string
        if (delimiter == displayIdentifier || delimiter == displayIdentifier + strlen(displayIdentifier) - 1) return NULL;
        // Splitting display identifier into identification method and value
        *delimiter = '\0';
        identificationMethod = displayIdentifier;
        displayIdentifier = delimiter + 1;
    }

    // Searching for display that matchs the identifier for the given identification method
    for (int i = 0; i < connectedDisplays; i++) {
        const char *displayValue = getDisplayIdentifier(displays + i, identificationMethod).UTF8String;
        if (displayValue != NULL && STR_EQ(displayIdentifier, displayValue)) {
            return displays + i;
        }
    }
    return NULL;
}

IOAVServiceRef getDefaultDisplayAVService() {
    return IOAVServiceCreate(kCFAllocatorDefault);
}

static Boolean proxyMatchesDisplay(io_service_t proxy, io_service_t displayController,
                                   io_service_t commonParent, const char *displayName) {
    io_registry_entry_t parent = MACH_PORT_NULL;
    if (IORegistryEntryGetParentEntry(proxy, kIOServicePlane, &parent) != KERN_SUCCESS) return false;
    io_name_t endpoint;
    if (IORegistryEntryGetName(parent, endpoint) != KERN_SUCCESS) { IOObjectRelease(parent); return false; }
    io_registry_entry_t current = parent;
    Boolean matches = false;
    while (current != MACH_PORT_NULL) {
        io_registry_entry_t next = MACH_PORT_NULL;
        if (IORegistryEntryGetParentEntry(current, kIOServicePlane, &next) != KERN_SUCCESS) {
            IOObjectRelease(current); break;
        }
        if (IOObjectIsEqualTo(next, commonParent)) {
            io_name_t controller;
            matches = !IOObjectIsEqualTo(current, displayController) &&
                IORegistryEntryGetName(current, controller) == KERN_SUCCESS &&
                displayRouteNamesMatch(displayName, endpoint, controller);
            IOObjectRelease(current); IOObjectRelease(next); break;
        }
        IOObjectRelease(current); current = next;
    }
    return matches;
}

DDCTransport getDisplayDDCTransport(DisplayInfos* displayInfos) {
    DDCTransport transport = {.service = NULL, .chipAddress = DDC_CHIP_ADDRESS_DEFAULT};
    if (displayInfos == NULL || displayInfos->adapter == MACH_PORT_NULL) return transport;
    io_registry_entry_t controller = MACH_PORT_NULL, commonParent = MACH_PORT_NULL;
    if (IORegistryEntryGetParentEntry(displayInfos->adapter, kIOServicePlane, &controller) != KERN_SUCCESS) return transport;
    io_name_t controllerName;
    if (IORegistryEntryGetName(controller, controllerName) != KERN_SUCCESS ||
        IORegistryEntryGetParentEntry(controller, kIOServicePlane, &commonParent) != KERN_SUCCESS) {
        IOObjectRelease(controller); return transport;
    }
    io_iterator_t iter = MACH_PORT_NULL;
    kern_return_t result = IOServiceGetMatchingServices(kIOMainPortDefault, IOServiceMatching("DCPAVServiceProxy"), &iter);
    if (result != KERN_SUCCESS || iter == MACH_PORT_NULL) {
        IOObjectRelease(commonParent); IOObjectRelease(controller); return transport;
    }
    unsigned matches = 0;
    io_service_t service;
    while ((service = IOIteratorNext(iter)) != MACH_PORT_NULL) {
        CFTypeRef location = IORegistryEntryCreateCFProperty(service, CFSTR("Location"), kCFAllocatorDefault, 0);
        Boolean external = location != NULL && CFGetTypeID(location) == CFStringGetTypeID() &&
            CFEqual(location, CFSTR("External"));
        if (location) CFRelease(location);
        if (external && proxyMatchesDisplay(service, controller, commonParent, controllerName)) {
            matches++;
            if (matches == 1) {
                transport.service = IOAVServiceCreateWithService(kCFAllocatorDefault, service);
                transport.chipAddress = isMCDP29XXProxy(service) ? DDC_CHIP_ADDRESS_MCDP29XX : DDC_CHIP_ADDRESS_DEFAULT;
            }
        }
        IOObjectRelease(service);
    }
    if (matches != 1 || !IOIteratorIsValid(iter)) {
        if (transport.service) CFRelease(transport.service);
        transport.service = NULL;
    }
    IOObjectRelease(iter); IOObjectRelease(commonParent); IOObjectRelease(controller);
    return transport;
}

IOAVServiceRef getDisplayAVService(DisplayInfos* displayInfos) {
    return getDisplayDDCTransport(displayInfos).service;
}
