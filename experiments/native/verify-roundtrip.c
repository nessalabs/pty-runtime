#define main lifecycle_main
#include "native-memory.c"
#undef main

static void reply(GhosttyTerminal t,void *ctx,const uint8_t *p,size_t n){(void)t;hash_write(ctx,p,n);}
static void enable_replies(GhosttyTerminal t,Hash *h){CHECK(ghostty_terminal_set(t,GHOSTTY_TERMINAL_OPT_USERDATA,h));CHECK(ghostty_terminal_set(t,GHOSTTY_TERMINAL_OPT_WRITE_PTY,(const void*)reply));}
static GhosttyTerminal decode_buffer(const uint8_t *p,size_t n){
    GhosttySnapshotDecoder d=NULL;GhosttyTerminal t=NULL;CHECK(ghostty_snapshot_decoder_new_buf(native_allocator(),&d,p,n));
    size_t cap=64*1024;bool retain=true;CHECK(ghostty_snapshot_decoder_set(d,GHOSTTY_SNAPSHOT_DECODER_OPT_MAX_CONTINUATION_BYTES,&cap));CHECK(ghostty_snapshot_decoder_set(d,GHOSTTY_SNAPSHOT_DECODER_OPT_RETAIN_CONTINUATION,&retain));
    CHECK(ghostty_snapshot_decoder_decode(d,&t));ghostty_snapshot_decoder_free(d);return t;
}
static void same(GhosttyTerminal a,GhosttyTerminal b){Hash x=formatted_hash(a),y=formatted_hash(b);assert(x.hash==y.hash&&x.len==y.len);}
int main(void){
    cap_mib=8;lines=10000;varied=1;allocator_mode=2;size_t corpus_len;uint8_t *corpus=make_corpus(&corpus_len);
    struct {const char *name,*prefix,*suffix;} cases[]={
        {"ground","","next\r\n"},
        {"utf8","\xe2\x82","\xac currency\r\n"},
        {"csi","\033[38;2;80;","100;200mstyled\033[0m\r\n"},
        {"osc","\033]2;partial title"," finished\033\\visible\r\n"},
        {"dcs","\033P1;2qpayload"," more\033\\visible\r\n"},
        {"alternate","\033[?1049h\033[?1000h\033[31","malternate\033[?1049lprimary\r\n"}
    };
    for(size_t i=0;i<sizeof(cases)/sizeof(cases[0]);i++){
        GhosttyTerminal a=new_terminal();feed(a,corpus,corpus_len);feed(a,(const uint8_t*)cases[i].prefix,strlen(cases[i].prefix));
        uint8_t *bytes=NULL;size_t len=0;CHECK(ghostty_snapshot_encode_alloc(a,native_allocator(),&bytes,&len));
        GhosttyTerminal b=decode_buffer(bytes,len);same(a,b);
        uint8_t *roundtrip=NULL;size_t roundtrip_len=0;CHECK(ghostty_snapshot_encode_alloc(b,native_allocator(),&roundtrip,&roundtrip_len));assert(roundtrip_len==len&&!memcmp(roundtrip,bytes,len));ghostty_free(native_allocator(),roundtrip,roundtrip_len);
        for(int cycle=0;cycle<10;cycle++){
            uint8_t *copy=NULL;size_t copy_len=0;CHECK(ghostty_snapshot_encode_alloc(b,native_allocator(),&copy,&copy_len));ghostty_terminal_free(b);b=decode_buffer(copy,copy_len);ghostty_free(native_allocator(),copy,copy_len);same(a,b);
        }
        Hash ra={1469598103934665603ULL,0},rb=ra;enable_replies(a,&ra);enable_replies(b,&rb);
        feed(a,(const uint8_t*)cases[i].suffix,strlen(cases[i].suffix));feed(b,(const uint8_t*)cases[i].suffix,strlen(cases[i].suffix));same(a,b);
        CHECK(ghostty_terminal_resize(a,100,30,8,16));CHECK(ghostty_terminal_resize(b,100,30,8,16));same(a,b);
        const uint8_t query[]="\033[6n";feed(a,query,sizeof(query)-1);feed(b,query,sizeof(query)-1);assert(ra.len>0&&ra.hash==rb.hash&&ra.len==rb.len);
        GhosttySnapshotDecoder d=NULL;GhosttyTerminal bad=NULL;CHECK(ghostty_snapshot_decoder_new_buf(native_allocator(),&d,bytes,len-1));assert(ghostty_snapshot_decoder_decode(d,&bad)!=GHOSTTY_SUCCESS&&bad==NULL);ghostty_snapshot_decoder_free(d);
        bytes[20]^=1;CHECK(ghostty_snapshot_decoder_new_buf(native_allocator(),&d,bytes,len));assert(ghostty_snapshot_decoder_decode(d,&bad)!=GHOSTTY_SUCCESS&&bad==NULL);ghostty_snapshot_decoder_free(d);
        ghostty_free(native_allocator(),bytes,len);ghostty_terminal_free(a);ghostty_terminal_free(b);assert(tracked_bytes==0&&mapped_bytes==0);
        printf("{\"case\":\"%s\",\"roundtrip\":true,\"binary_roundtrip_equal\":true,\"additional_cycles\":10,\"resume_and_resize\":true,\"replies\":true,\"truncated_and_corrupt_rejected\":true}\n",cases[i].name);
    }
    free(corpus);return 0;
}
