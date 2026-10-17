@import Foundation;
#include "i2c.h"
#include <assert.h>
#include <stdio.h>
// Parser tests must never contact a display.
IOReturn IOAVServiceReadI2C(IOAVServiceRef s,uint32_t c,uint32_t o,void *b,uint32_t n) { (void)s;(void)c;(void)o;(void)b;(void)n; abort(); }
IOReturn IOAVServiceWriteI2C(IOAVServiceRef s,uint32_t c,uint32_t o,void *b,uint32_t n) { (void)s;(void)c;(void)o;(void)b;(void)n; abort(); }
static void checksum(UInt8 *p) { p[10]=0x50; for(int i=0;i<10;i++)p[10]^=p[i]; }
int main(void) { @autoreleasepool {
 DDCPacket request=createDDCPacket(INPUT);prepareDDCRead(&request);
 const UInt8 expected[]={0x82,0x01,0x60,0xdc};assert(memcmp(request.data,expected,4)==0);
 prepareDDCRead(&request);assert(memcmp(request.data,expected,4)==0);
 DDCPacket alternate=createDDCPacket(INPUT_ALT);prepareDDCRead(&alternate);
 const UInt8 alternateExpected[]={0x82,0x01,0xf4,0x49};assert(memcmp(alternate.data,alternateExpected,4)==0);
 UInt8 nullReply[12]={0x6e,0x80,0xbe,0,0,0,0,0,0xec,0x03,0,0};assert(convertI2CtoDDC(nullReply,12,0x60).curValue==-1);
 UInt8 valid[12]={0x6e,0x88,0x02,0,0x60,0,0,0x12,0,0x11,0,0}; checksum(valid);
 DDCValue value=convertI2CtoDDC(valid,sizeof(valid),0x60);
 assert(value.curValue==17 && value.maxValue==18);
 assert(convertI2CtoDDC(valid,10,0x60).curValue==-1);
 assert(convertI2CtoDDC(NULL,12,0x60).curValue==-1);
 assert(convertI2CtoDDC(valid,12,0x10).curValue==-1);
 const UInt8 offsets[]={0,1,2,3,4,5};
 const UInt8 malformed[]={0x50,0x87,0x01,1,0x10,2};
 for(size_t i=0;i<sizeof(offsets);i++){UInt8 p[12];memcpy(p,valid,12);p[offsets[i]]=malformed[i];checksum(p);assert(convertI2CtoDDC(p,12,0x60).curValue==-1);}
 UInt8 broken[12];memcpy(broken,valid,12);broken[10]^=1;assert(convertI2CtoDDC(broken,12,0x60).curValue==-1);
 UInt8 empty[12]={0};assert(convertI2CtoDDC(empty,12,0x60).curValue==-1);
 UInt8 garbage[12];memset(garbage,0xec,12);garbage[9]=3;assert(convertI2CtoDDC(garbage,12,0x60).curValue==-1);
 UInt8 big[12];memcpy(big,valid,12);big[6]=0x12;big[7]=0x34;big[8]=0xab;big[9]=0xcd;checksum(big);
 value=convertI2CtoDDC(big,12,0x60);assert(value.maxValue==0x1234 && value.curValue==0xabcd);
 puts("DDC reply validation: 18 checks passed (synthetic; no hardware access)");
} }
