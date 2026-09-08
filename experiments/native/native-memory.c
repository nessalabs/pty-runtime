#include <assert.h>
#include <errno.h>
#include <fcntl.h>
#include <inttypes.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/resource.h>
#include <sys/mman.h>
#include <sys/stat.h>
#include <time.h>
#include <unistd.h>
#include <ghostty/vt.h>
#include "platform.h"

#define CHECK(x) do { GhosttyResult r_=(x); if(r_!=GHOSTTY_SUCCESS){fprintf(stderr,"%s:%d result=%d: %s\n",__FILE__,__LINE__,r_,#x);exit(2);} } while(0)
static int count, lines, varied, compression, cap_mib;
static int allocator_mode;
static size_t tracked_bytes,mapped_bytes,mapped_allocations;
static size_t page_round(size_t n){size_t p=(size_t)getpagesize();return(n+p-1)/p*p;}
static bool use_mapping(size_t n){return allocator_mode>=2&&n>=(allocator_mode==3?4096:16384);}
static void *custom_alloc(void *ctx,size_t n,uint8_t align,uintptr_t ra){
    (void)ctx;(void)ra;assert(align<=16);void *p;
    if(use_mapping(n)){size_t bytes=page_round(n);p=mmap(NULL,bytes,PROT_READ|PROT_WRITE,MAP_PRIVATE|MAP_ANON,-1,0);if(p==MAP_FAILED)return NULL;mapped_bytes+=bytes;mapped_allocations++;}
    else p=malloc(n);
    if(p)tracked_bytes+=n;return p;
}
static bool custom_resize(void *ctx,void *p,size_t n,uint8_t a,size_t new_n,uintptr_t ra){(void)ctx;(void)p;(void)n;(void)a;(void)new_n;(void)ra;return false;}
static void *custom_remap(void *ctx,void *p,size_t n,uint8_t a,size_t new_n,uintptr_t ra){(void)ctx;(void)p;(void)n;(void)a;(void)new_n;(void)ra;return NULL;}
static void custom_free(void *ctx,void *p,size_t n,uint8_t a,uintptr_t ra){
    (void)ctx;(void)a;(void)ra;assert(tracked_bytes>=n);tracked_bytes-=n;
    if(use_mapping(n)){size_t bytes=page_round(n);assert(munmap(p,bytes)==0);mapped_bytes-=bytes;}else free(p);
}
static const GhosttyAllocatorVtable alloc_vtable={custom_alloc,custom_resize,custom_remap,custom_free};
static const GhosttyAllocator alloc_context={NULL,&alloc_vtable};
static const GhosttyAllocator *native_allocator(void){return allocator_mode?&alloc_context:NULL;}
static uint64_t now_ns(void){return platform_now_ns();}
static double cpu_ms(void){struct rusage r;assert(getrusage(RUSAGE_SELF,&r)==0);return (r.ru_utime.tv_sec+r.ru_stime.tv_sec)*1000.0+(r.ru_utime.tv_usec+r.ru_stime.tv_usec)/1000.0;}
static void stage(const char *name){
    printf("{\"kind\":\"memory\",\"stage\":\"%s\",\"n\":%d,\"lines\":%d,\"varied\":%d,\"compression\":%d,\"cap_mib\":%d,",name,count,lines,varied,compression,cap_mib);
    platform_memory_json();
    printf(",\"allocator_mode\":%d,\"tracked_native_bytes\":%zu,\"mapped_allocator_bytes\":%zu,\"mapped_allocations\":%zu}\n",allocator_mode,tracked_bytes,mapped_bytes,mapped_allocations);
}
static bool file_write(void *p,const uint8_t *b,size_t n){return fwrite(b,1,n,p)==n;}
static bool file_read(void *p,uint8_t *b,size_t n,size_t *out){*out=fread(b,1,n,p);return !ferror((FILE*)p);}
typedef struct {uint64_t hash;size_t len;} Hash;
static bool hash_write(void *p,const uint8_t *b,size_t n){Hash *h=p;for(size_t i=0;i<n;i++){h->hash^=b[i];h->hash*=1099511628211ULL;}h->len+=n;return true;}
static Hash formatted_hash(GhosttyTerminal t){
    GhosttyFormatter f=NULL;
    GhosttyFormatterTerminalOptions o={.size=sizeof(o),.emit=GHOSTTY_FORMATTER_FORMAT_VT,.unwrap=true,.trim=true};
    o.extra=(GhosttyFormatterTerminalExtra){.size=sizeof(o.extra),.palette=true,.modes=true,.scrolling_region=true,.tabstops=true,.pwd=true,.keyboard=true};
    o.extra.screen=(GhosttyFormatterScreenExtra){.size=sizeof(o.extra.screen),.cursor=true,.style=true,.hyperlink=true,.protection=true,.kitty_keyboard=true,.charsets=true};
    CHECK(ghostty_formatter_terminal_new(native_allocator(),&f,t,o));
    Hash h={1469598103934665603ULL,0};CHECK(ghostty_formatter_format(f,(GhosttyWriter){hash_write,&h}));ghostty_formatter_free(f);return h;
}
static GhosttyTerminal new_terminal(void){
    GhosttyTerminal t=NULL;CHECK(ghostty_terminal_new(native_allocator(),&t,80,24));
    size_t cap=(size_t)cap_mib*1024*1024,cont=64*1024;
    CHECK(ghostty_terminal_set(t,GHOSTTY_TERMINAL_OPT_SCROLLBACK_MAX_BYTES,&cap));
    CHECK(ghostty_terminal_set(t,GHOSTTY_TERMINAL_OPT_CONTINUATION_MAX_BYTES,&cont));
    size_t images=0;CHECK(ghostty_terminal_set(t,GHOSTTY_TERMINAL_OPT_KITTY_IMAGE_STORAGE_LIMIT,&images));
    return t;
}
static uint8_t *make_corpus(size_t *len){
    *len=(size_t)lines*78;uint8_t *p=malloc(*len?*len:1);assert(p);uint32_t rng=1234567;
    for(int l=0;l<lines;l++){
        uint8_t *b=p+(size_t)l*78;
        for(int c=0;c<76;c++){
            rng^=rng<<13;rng^=rng>>17;rng^=rng<<5;
            b[c]=varied?('!'+rng%90):"build step completed: cached artifact, elapsed 120 ms; "[c%54];
        }
        b[76]='\r';b[77]='\n';
    }return p;
}
static void feed(GhosttyTerminal t,const uint8_t *p,size_t n){for(size_t off=0;off<n;){size_t k=n-off;if(k>16384)k=16384;ghostty_terminal_vt_write(t,p+off,k);off+=k;}}
static void check_continuation(GhosttyTerminal t){uint8_t b[32];size_t n=0;CHECK(ghostty_terminal_continuation_buf(t,b,sizeof(b),&n));assert(n==4&&!memcmp(b,"\033[31",4));}
static int cmp_double(const void*a,const void*b){double x=*(const double*)a,y=*(const double*)b;return(x>y)-(x<y);}
static double percentile(double *v,int n,double p){qsort(v,n,sizeof(*v),cmp_double);return v[(int)((n-1)*p)];}
int main(int argc,char **argv){
    assert(argc>=6&&argc<=8);count=atoi(argv[1]);lines=atoi(argv[2]);varied=atoi(argv[3]);compression=atoi(argv[4]);cap_mib=atoi(argv[5]);int relief=(argc>=7)?atoi(argv[6]):0;allocator_mode=(argc==8)?atoi(argv[7]):0;assert(count>0&&count<=512&&lines>=0&&cap_mib>0&&allocator_mode>=0&&allocator_mode<=3);
    setvbuf(stdout,NULL,_IONBF,0);umask(0077);
    size_t corpus_len=0;uint8_t *corpus=make_corpus(&corpus_len);
    GhosttyTerminal *terms=calloc(count,sizeof(*terms));GhosttySnapshotDecoder *dec=calloc(count,sizeof(*dec));FILE **files=calloc(count,sizeof(*files));
    double *ready=calloc(count,sizeof(double)),*history=calloc(count,sizeof(double));assert(terms&&dec&&files&&ready&&history);
    char dirname[]="native-snapshots-XXXXXX";assert(mkdtemp(dirname));char path[256];
    stage("baseline");
    for(int i=0;i<count;i++)terms[i]=new_terminal();stage("empty");
    uint64_t begin=now_ns();double cpu0=cpu_ms();
    for(int i=0;i<count;i++){feed(terms[i],corpus,corpus_len);ghostty_terminal_vt_write(terms[i],(const uint8_t*)"\033[31",4);}
    double feed_ms=(now_ns()-begin)/1e6,feed_cpu=cpu_ms()-cpu0;stage("filled");
    Hash before=formatted_hash(terms[0]);check_continuation(terms[0]);
    uint64_t steps=0;double compression_ms=0,compression_cpu=0,max_step_us=0;
    if(compression){
        begin=now_ns();cpu0=cpu_ms();
        for(int i=0;i<count;i++){
            GhosttyTerminalCompressionResult result;
            do{uint64_t a=now_ns();CHECK(ghostty_terminal_compress(terms[i],GHOSTTY_TERMINAL_COMPRESSION_MODE_INCREMENTAL,&result));double d=(now_ns()-a)/1e3;if(d>max_step_us)max_step_us=d;steps++;assert(result!=GHOSTTY_TERMINAL_COMPRESSION_RESULT_UNSUPPORTED);assert(steps<1000000);}while(result==GHOSTTY_TERMINAL_COMPRESSION_RESULT_PENDING);
        }compression_ms=(now_ns()-begin)/1e6;compression_cpu=cpu_ms()-cpu0;
    }stage("after_compression");
    uint64_t disk_bytes=0;double encode_ms=0,fsync_ms=0;cpu0=cpu_ms();
    for(int i=0;i<count;i++){
        snprintf(path,sizeof(path),"%s/%d.bin",dirname,i);int fd=open(path,O_RDWR|O_CREAT|O_EXCL,0600);assert(fd>=0);FILE *f=fdopen(fd,"w+b");assert(f);
        begin=now_ns();CHECK(ghostty_snapshot_encode(terms[i],(GhosttyWriter){file_write,f}));assert(fflush(f)==0);encode_ms+=(now_ns()-begin)/1e6;
        struct stat s;assert(fstat(fd,&s)==0);disk_bytes+=s.st_size;
        begin=now_ns();assert(fsync(fd)==0);fsync_ms+=(now_ns()-begin)/1e6;assert(fclose(f)==0);
    }double encode_cpu=cpu_ms()-cpu0;stage("encoded");
    for(int i=0;i<count;i++){ghostty_terminal_free(terms[i]);terms[i]=NULL;}stage("parked");
    usleep(200000);stage("parked_settled");
    if(relief){begin=now_ns();size_t relieved=platform_allocator_relief();double relief_us=(now_ns()-begin)/1e3;stage("parked_after_allocator_relief");printf("{\"kind\":\"reclamation\",\"n\":%d,\"varied\":%d,\"compression\":%d,\"allocator_relief_return\":%zu,\"relief_us\":%.3f}\n",count,varied,compression,relieved,relief_us);}
    size_t ready_bytes=0;cpu0=cpu_ms();
    for(int i=0;i<count;i++){
        begin=now_ns();snprintf(path,sizeof(path),"%s/%d.bin",dirname,i);files[i]=fopen(path,"rb");assert(files[i]);
        CHECK(ghostty_snapshot_decoder_new(native_allocator(),&dec[i],(GhosttyReader){file_read,files[i]}));
        size_t cont=64*1024;bool retain=true;CHECK(ghostty_snapshot_decoder_set(dec[i],GHOSTTY_SNAPSHOT_DECODER_OPT_MAX_CONTINUATION_BYTES,&cont));CHECK(ghostty_snapshot_decoder_set(dec[i],GHOSTTY_SNAPSHOT_DECODER_OPT_RETAIN_CONTINUATION,&retain));
        CHECK(ghostty_snapshot_decoder_ready(dec[i],&terms[i]));ready[i]=(now_ns()-begin)/1e3;
        size_t off=0;CHECK(ghostty_snapshot_decoder_get(dec[i],GHOSTTY_SNAPSHOT_DECODER_DATA_SOURCE_OFFSET,&off));ready_bytes+=off;
    }double ready_cpu=cpu_ms()-cpu0;stage("ready");
    uint64_t pages=0,rows=0,skipped=0;cpu0=cpu_ms();
    for(int i=0;i<count;i++){
        begin=now_ns();GhosttyResult r;
        while((r=ghostty_snapshot_decoder_next(dec[i]))==GHOSTTY_SUCCESS){size_t added=0;CHECK(ghostty_snapshot_decoder_get(dec[i],GHOSTTY_SNAPSHOT_DECODER_DATA_PROGRESS_ROWS,&added));rows+=added;pages++;if(!added)skipped++;}
        assert(r==GHOSTTY_NO_VALUE);history[i]=(now_ns()-begin)/1e3;
        ghostty_snapshot_decoder_free(dec[i]);assert(fclose(files[i])==0);
    }double history_cpu=cpu_ms()-cpu0;stage("restored");
    for(int i=0;i<count;i++){Hash h=formatted_hash(terms[i]);assert(h.hash==before.hash&&h.len==before.len);check_continuation(terms[i]);}
    for(int i=0;i<count;i++){ghostty_terminal_free(terms[i]);snprintf(path,sizeof(path),"%s/%d.bin",dirname,i);assert(unlink(path)==0);}assert(rmdir(dirname)==0);stage("cleanup");
    double ready_sum=0,history_sum=0;for(int i=0;i<count;i++){ready_sum+=ready[i];history_sum+=history[i];}
    printf("{\"kind\":\"summary\",\"n\":%d,\"lines\":%d,\"varied\":%d,\"compression\":%d,\"cap_mib\":%d,\"corpus_bytes\":%zu,\"feed_ms\":%.6f,\"feed_cpu_ms\":%.6f,\"compression_ms\":%.6f,\"compression_cpu_ms\":%.6f,\"compression_steps\":%" PRIu64 ",\"max_compression_step_us\":%.3f,\"encode_file_ms\":%.6f,\"fsync_ms\":%.6f,\"encode_cpu_ms\":%.6f,\"snapshot_bytes\":%" PRIu64 ",\"ready_source_bytes\":%zu,\"ready_sum_us\":%.3f,\"ready_p50_us\":%.3f,\"ready_p99_us\":%.3f,\"ready_max_us\":%.3f,\"ready_cpu_ms\":%.6f,\"history_sum_us\":%.3f,\"history_p50_us\":%.3f,\"history_p99_us\":%.3f,\"history_cpu_ms\":%.6f,\"history_pages\":%" PRIu64 ",\"history_rows\":%" PRIu64 ",\"skipped_pages\":%" PRIu64 ",\"formatted_bytes_per_terminal\":%zu,\"verified_terminals\":%d}\n",count,lines,varied,compression,cap_mib,corpus_len,feed_ms,feed_cpu,compression_ms,compression_cpu,steps,max_step_us,encode_ms,fsync_ms,encode_cpu,disk_bytes,ready_bytes,ready_sum,percentile(ready,count,.5),percentile(ready,count,.99),percentile(ready,count,1),ready_cpu,history_sum,percentile(history,count,.5),percentile(history,count,.99),history_cpu,pages,rows,skipped,before.len,count);
    assert(tracked_bytes==0&&mapped_bytes==0);free(corpus);free(terms);free(dec);free(files);free(ready);free(history);return 0;
}
