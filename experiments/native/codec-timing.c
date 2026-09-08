#define main lifecycle_main
#include "native-memory.c"
#undef main

static void decoder_init(GhosttySnapshotDecoder *d,uint8_t *bytes,size_t n){
    CHECK(ghostty_snapshot_decoder_new_buf(NULL,d,bytes,n));
    bool retain=true;size_t cap=64*1024;
    CHECK(ghostty_snapshot_decoder_set(*d,GHOSTTY_SNAPSHOT_DECODER_OPT_MAX_CONTINUATION_BYTES,&cap));
    CHECK(ghostty_snapshot_decoder_set(*d,GHOSTTY_SNAPSHOT_DECODER_OPT_RETAIN_CONTINUATION,&retain));
}
int main(int argc,char **argv){
    assert(argc==3);lines=atoi(argv[1]);varied=atoi(argv[2]);cap_mib=128;
    size_t corpus_len;uint8_t *corpus=make_corpus(&corpus_len);GhosttyTerminal source=new_terminal();feed(source,corpus,corpus_len);feed(source,(const uint8_t*)"\033[31",4);
    uint8_t *bytes=NULL;size_t n=0;CHECK(ghostty_snapshot_encode_alloc(source,NULL,&bytes,&n));
    uint8_t *encoded=malloc(n);assert(encoded);size_t ready_bytes=0;
    for(int i=-5;i<30;i++){
        uint64_t begin=now_ns();size_t wrote=0;CHECK(ghostty_snapshot_encode_buf(source,encoded,n,&wrote));double encode_us=(now_ns()-begin)/1e3;assert(wrote==n);
        GhosttySnapshotDecoder d=NULL;GhosttyTerminal t=NULL;
        begin=now_ns();decoder_init(&d,bytes,n);CHECK(ghostty_snapshot_decoder_ready(d,&t));double ready_us=(now_ns()-begin)/1e3;
        CHECK(ghostty_snapshot_decoder_get(d,GHOSTTY_SNAPSHOT_DECODER_DATA_SOURCE_OFFSET,&ready_bytes));
        begin=now_ns();feed(t,(const uint8_t*)"mW",2);double first_feed_us=(now_ns()-begin)/1e3;
        ghostty_snapshot_decoder_free(d);ghostty_terminal_free(t);
        begin=now_ns();decoder_init(&d,bytes,n);CHECK(ghostty_snapshot_decoder_decode(d,&t));double full_us=(now_ns()-begin)/1e3;
        check_continuation(t);ghostty_snapshot_decoder_free(d);ghostty_terminal_free(t);
        if(i>=0)printf("{\"kind\":\"codec\",\"lines\":%d,\"varied\":%d,\"sample\":%d,\"snapshot_bytes\":%zu,\"ready_bytes\":%zu,\"encode_us\":%.3f,\"ready_us\":%.3f,\"first_feed_us\":%.3f,\"full_decode_us\":%.3f}\n",lines,varied,i,n,ready_bytes,encode_us,ready_us,first_feed_us,full_us);
    }
    free(corpus);free(encoded);ghostty_free(NULL,bytes,n);ghostty_terminal_free(source);return 0;
}
