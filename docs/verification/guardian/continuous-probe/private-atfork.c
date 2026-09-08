#include <errno.h>
#include <pthread.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <unistd.h>
extern char *__llvm_profile_begin_counters(void);
extern char *__llvm_profile_end_counters(void);
extern int __llvm_profile_set_file_object(FILE *, int);
extern const char profile_directory[];
static void *saved;
static size_t size, mapped_size;
static void fail(void) { write(2, "coverage atfork failed\n", 23); _exit(126); }
static void prepare(void) {
  char *begin = __llvm_profile_begin_counters();
  size = __llvm_profile_end_counters() - begin;
  size_t page = (size_t)sysconf(_SC_PAGESIZE);
  mapped_size = (size + page - 1) / page * page;
  saved = malloc(size);
  if (!saved) fail();
  memcpy(saved, begin, size);
}
static void parent(void) { free(saved); saved = NULL; }
static void child(void) {
  char *begin = __llvm_profile_begin_counters();
  if (mmap(begin, mapped_size, PROT_READ | PROT_WRITE,
           MAP_FIXED | MAP_PRIVATE | MAP_ANON, -1, 0) != begin) fail();
  memcpy(begin, saved, size);
  free(saved); saved = NULL;
  char filename[4096];
  int n = snprintf(filename, sizeof(filename), "%s/fork-%ld.profraw", profile_directory, (long)getpid());
  if (n < 0 || n >= (int)sizeof(filename)) fail();
  FILE *file = fopen(filename, "w+");
  if (!file || __llvm_profile_set_file_object(file, 1)) fail();
  if (fclose(file)) fail();
}
__attribute__((constructor)) static void setup(void) {
  if (pthread_atfork(prepare, parent, child)) fail();
}
