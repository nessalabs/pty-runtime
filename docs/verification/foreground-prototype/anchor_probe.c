// Isolated fresh-exec PTY guardian experiment, not production runtime code.
#define _GNU_SOURCE
#include <sys/types.h>
#include <sys/socket.h>
#include <sys/wait.h>
#include <sys/ioctl.h>
#include <termios.h>
#include <unistd.h>
#include <signal.h>
#include <poll.h>
#include <fcntl.h>
#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#ifdef __APPLE__
#include <util.h>
#else
#include <pty.h>
#endif
struct report { int mode, joined, same_session, foreground, term_seen, killed, background_alive, workload_pid, workload_status, guardian_pid, anchor_status; };
static void die(const char *message) { perror(message); _exit(111); }
static void exact_write(int fd, const void *bytes, size_t n) {
    const char *p=bytes;
    while(n) { ssize_t k=write(fd,p,n); if(k<0 && errno==EINTR)continue; if(k<=0)die("write"); p+=k;n-=(size_t)k; }
}
static void exact_read(int fd, void *bytes, size_t n) {
    char *p=bytes;
    while(n) { struct pollfd f={fd,POLLIN,0}; if(poll(&f,1,3000)!=1)die("read deadline"); ssize_t k=read(fd,p,n); if(k<0&&errno==EINTR)continue; if(k<=0)die("read"); p+=k;n-=(size_t)k; }
}
static int wait_child(pid_t pid) { int status; while(waitpid(pid,&status,0)<0) { if(errno!=EINTR)die("waitpid"); } return status; }
static void terminate(int signal_number) { (void)signal_number; _exit(17); }
static pid_t sleeper(pid_t group, int cooperative) {
    int ready[2];if(pipe(ready))die("pipe");
    pid_t pid=fork(); if(pid<0)die("fork");
    if(pid==0) {
        close(ready[0]); if(setpgid(0,group))die("setpgid sleeper");
        signal(SIGTERM,cooperative?terminate:SIG_IGN); signal(SIGHUP,SIG_IGN);
        exact_write(ready[1],"R",1); close(ready[1]); for(;;)pause();
    }
    close(ready[1]);char mark;exact_read(ready[0],&mark,1);close(ready[0]);return pid;
}
static void helper(int mode) {
    if(setsid()<0 || ioctl(0,TIOCSCTTY,0)<0)die("controlling terminal");
    signal(SIGTTOU,SIG_IGN);signal(SIGTTIN,SIG_IGN);signal(SIGHUP,SIG_IGN);
    const pid_t sid=getsid(0);
    struct report report={0};report.mode=mode;report.guardian_pid=getpid();
    int workpipe[2];if(pipe(workpipe))die("workpipe");
    pid_t workload=fork();if(workload<0)die("workload fork");
    if(!workload) {close(workpipe[1]);if(setpgid(0,0))die("workgroup");char c;exact_read(workpipe[0],&c,1);_exit(42);}
    close(workpipe[0]);report.workload_pid=workload;
    pid_t background=sleeper(0,0);
    pid_t leader=sleeper(0,mode==0);pid_t target=leader;
    if(mode==2) { target=sleeper(leader,0);kill(leader,SIGKILL);(void)wait_child(leader); }
    if(mode==4) { kill(leader,SIGKILL);(void)wait_child(leader);leader=getpgid(getppid()); }
    if(mode!=4 && tcsetpgrp(0,leader))die("foreground");
    if(mode==5 && tcsetpgrp(0,background))die("switch foreground");
    if(mode==3) {kill(leader,SIGKILL);(void)wait_child(leader);}
    int command[2],status[2];if(pipe(command)||pipe(status))die("anchor pipes");
    pid_t anchor=fork();if(anchor<0)die("anchor fork");
    if(!anchor) {
        close(command[1]);close(status[0]);signal(SIGTERM,SIG_IGN);signal(SIGHUP,SIG_IGN);signal(SIGTSTP,SIG_IGN);
        int verified[3]={0};
        verified[0]=setpgid(0,leader)==0;
        verified[1]=verified[0]&&getsid(0)==sid;
        verified[2]=verified[1]&&tcgetpgrp(0)==getpgrp();
        exact_write(status[1],verified,sizeof verified);
        if(!verified[2])_exit(29);
        char action;exact_read(command[0],&action,1);
        if(action!='T')_exit(112);
        // No numeric group lookup: the caller is a verified member of the target.
        if(kill(0,SIGTERM))die("own group TERM");
        exact_write(status[1],"T",1);
        exact_read(command[0],&action,1);
        if(action=='K') {kill(0,SIGKILL);_exit(113);} _exit(0);
    }
    close(command[0]);close(status[1]);int verified[3];exact_read(status[0],verified,sizeof verified);
    report.joined=verified[0];report.same_session=verified[1];report.foreground=verified[2];
    if(mode>=3) {
        if(mode==5) {kill(target,SIGKILL);(void)wait_child(target);}
        report.anchor_status=wait_child(anchor);report.background_alive=kill(background,0)==0;
    } else {
        exact_write(command[1],"T",1);char mark;exact_read(status[0],&mark,1);
        if(mode==0) {int target_status=wait_child(target);report.term_seen=WIFEXITED(target_status)&&WEXITSTATUS(target_status)==17;exact_write(command[1],"X",1);}
        else {
            // The TERM-ignoring group stays pinned by the live anchor until KILL.
            usleep(10000);report.term_seen=kill(target,0)==0;
            exact_write(command[1],"K",1);int target_status=wait_child(target);report.killed=WIFSIGNALED(target_status)&&WTERMSIG(target_status)==SIGKILL;
        }
        report.anchor_status=wait_child(anchor);report.background_alive=kill(background,0)==0;
    }
    close(command[1]);close(status[0]);kill(background,SIGKILL);(void)wait_child(background);
    exact_write(workpipe[1],"F",1);close(workpipe[1]);report.workload_status=wait_child(workload);
    exact_write(3,&report,sizeof report);_exit(0);
}
int main(int argc,char **argv) {
    if(argc==3 && !strcmp(argv[1],"--helper"))helper(atoi(argv[2]));
    for(int mode=0;mode<6;mode++) {
        int host,child,channel[2];if(openpty(&host,&child,NULL,NULL,NULL)||socketpair(AF_UNIX,SOCK_STREAM,0,channel))die("endpoints");
        pid_t guardian=fork();if(guardian<0)die("guardian fork");
        if(!guardian) {
            close(host);close(channel[0]);int control=fcntl(channel[1],F_DUPFD,10);if(control<0)die("dup control");
            if(dup2(child,0)<0||dup2(child,1)<0||dup2(child,2)<0||dup2(control,3)<0)die("dup2");
            for(int fd=4;fd<64;fd++)close(fd);
            char mode_text[8];snprintf(mode_text,sizeof mode_text,"%d",mode);
            execl(argv[0],argv[0],"--helper",mode_text,(char*)NULL);die("exec helper");
        }
        close(child);close(channel[1]);struct report r;exact_read(channel[0],&r,sizeof r);int guardian_status=wait_child(guardian);
        int okay=r.background_alive&&r.workload_pid!=r.guardian_pid&&WIFEXITED(r.workload_status)&&WEXITSTATUS(r.workload_status)==42&&guardian_status==0;
        if(mode>=3)okay=okay&&(mode==5?r.joined&&!r.foreground:!r.joined)&&WIFEXITED(r.anchor_status)&&WEXITSTATUS(r.anchor_status)==29;
        else okay=okay&&r.joined&&r.same_session&&r.foreground&&r.term_seen&&(mode==0||r.killed);
        printf("{\"case\":%d,\"joined\":%d,\"same_session\":%d,\"foreground\":%d,\"term_seen\":%d,\"killed\":%d,\"background_alive\":%d,\"workload_pid\":%d,\"guardian_pid\":%d,\"workload_exit\":%d,\"pass\":%s}\n",mode,r.joined,r.same_session,r.foreground,r.term_seen,r.killed,r.background_alive,r.workload_pid,r.guardian_pid,WEXITSTATUS(r.workload_status),okay?"true":"false");
        close(host);close(channel[0]);if(!okay)return 1;
    }
    return 0;
}
