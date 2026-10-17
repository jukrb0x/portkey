@import Foundation;

#include "i2c.h"
#include "utils.h"

static int getBytesUsed(UInt8* data) {
    int bytes = 0;
    // Check up to DDC_BUFFER_SIZE bytes, not sizeof(data) which is just pointer size
    for (int i = 0; i < DDC_BUFFER_SIZE; ++i) {
        if (data[i] != 0) {
            bytes = i + 1;
        }
    }
    return bytes;
}

// Function to get ready for DDC operations for a specific display attribute
DDCPacket createDDCPacket(UInt8 attrCode) {
    DDCPacket packet = {};
    packet.data[2] = attrCode;
    packet.inputAddr = packet.data[2] == INPUT_ALT ? ALTERNATE_INPUT_ADDRESS : DEFAULT_INPUT_ADDRESS;
    return packet;
}

// Prepare DDC packet for read
void prepareDDCRead(DDCPacket *packet) {
    UInt8 *data = packet->data;
    data[0] = 0x82;
    data[1] = 0x01;
    data[3] = 0x6e ^ packet->inputAddr ^ data[0] ^ data[1] ^ data[2];
}

// Prepare DDC packet for write
void prepareDDCWrite(DDCPacket *packet, UInt16 newValue) {
    UInt8* data = packet->data;
    data[0] = 0x84;
    data[1] = 0x03;
    data[3] = (newValue) >> 8;
    data[4] = newValue & 255;
    data[5] = 0x6E ^ packet->inputAddr ^ data[0] ^ data[1] ^ data[2] ^ data[3] ^ data[4];
}


IOReturn performDDCReadAtChipAddress(IOAVServiceRef avService, UInt32 chipAddress, DDCPacket *packet) {
    memset(packet->data, 0, sizeof(UInt8) * DDC_BUFFER_SIZE);
    usleep(chipAddress == DDC_CHIP_ADDRESS_MCDP29XX ? DDC_MCDP_READ_WAIT : DDC_WAIT);
    return IOAVServiceReadI2C(avService, chipAddress, packet->inputAddr, packet->data, 12);
}

IOReturn performDDCWriteAtChipAddress(IOAVServiceRef avService, UInt32 chipAddress, DDCPacket *packet) {
    IOReturn ret;

    for (int i = 0; i < DDC_ITERATIONS; ++i) {
        usleep(DDC_WAIT);
        if ((ret = IOAVServiceWriteI2C(avService, chipAddress, packet->inputAddr, packet->data, getBytesUsed(packet->data)))) {
            return ret;
        }
    }
    return ret;
}

IOReturn performDDCRead(IOAVServiceRef avService, DDCPacket *packet) {
    return performDDCReadAtChipAddress(avService, DDC_CHIP_ADDRESS_DEFAULT, packet);
}

IOReturn performDDCWrite(IOAVServiceRef avService, DDCPacket *packet) {
    return performDDCWriteAtChipAddress(avService, DDC_CHIP_ADDRESS_DEFAULT, packet);
}


DDCValue convertI2CtoDDC(const UInt8 *bytes, size_t length, UInt8 expectedCode) {
    const DDCValue invalid = {-1, -1};
    // Full Get VCP reply is 11 bytes; IOAV requests 12 and may append padding.
    if (bytes == NULL || length < 11 || bytes[0] != 0x6e || bytes[1] != 0x88 ||
        bytes[2] != 0x02 || bytes[3] != 0x00 || bytes[4] != expectedCode || bytes[5] > 1) {
        return invalid;
    }
    UInt8 checksum = 0x50;
    for (size_t i = 0; i < 11; i++) checksum ^= bytes[i];
    if (checksum != 0) return invalid;
    DDCValue value = {
        .curValue = ((unsigned int)bytes[8] << 8) | bytes[9],
        .maxValue = ((unsigned int)bytes[6] << 8) | bytes[7],
    };
    return value;
}
