.section .text._start
.globl _start
_start:
    xor %ebp, %ebp
    mov (%rsp), %rax
    lea 16(%rsp,%rax,8), %r10
    mov %r10, .Lenvp(%rip)
    mov (%rsp), %rdi
    lea 8(%rsp), %rsi
    and $-16, %rsp
    call main
    mov %eax, %edi
    mov $60, %eax
    syscall

.section .text.calloc
.globl calloc
calloc:
    imul %rsi, %rdi
    mov %rdi, %rsi
    add $4095, %rsi
    and $-4096, %rsi
    xor %edi, %edi
    mov $3, %edx
    mov $0x22, %r10d
    mov $-1, %r8
    xor %r9d, %r9d
    mov $9, %eax
    syscall
    ret

.section .text.free
.globl free
free:
    ret

.section .text.memset
.globl memset
memset:
    mov %rdi, %rax
    mov %rsi, %rcx
    mov %rdx, %r8
    test %r8, %r8
    jz 2f
1:  mov %cl, (%rdi)
    inc %rdi
    dec %r8
    jnz 1b
2:  ret

.section .text.memcpy
.globl memcpy
memcpy:
    mov %rdi, %rax
    mov %rdx, %r8
    test %r8, %r8
    jz 2f
1:  mov (%rsi), %cl
    mov %cl, (%rdi)
    inc %rsi
    inc %rdi
    dec %r8
    jnz 1b
2:  ret

.section .text.exit
.globl exit
exit:
    mov $60, %eax
    syscall
    hlt

.section .text.strlen
.globl strlen
strlen:
    xor %rax, %rax
1:  cmpb $0, (%rdi,%rax)
    je 2f
    inc %rax
    jmp 1b
2:  ret

.section .bss
.p2align 4
.Lnumbuf:
    .zero 64

.section .text.printf
.globl printf
printf:
    push %r12
    push %r13
    push %r14
    mov %rdi, %r13
    mov %rsi, %r12
    mov %rsi, %r14
1:  movzbq (%r13), %rax
    test %rax, %rax
    jz 9f
    cmp $37, %al
    je 2f
    call .Lputc
    inc %r13
    jmp 1b
2:  inc %r13
    movzbq (%r13), %rax
    cmp $46, %al
    jne 7f
    cmpb $42, 1(%r13)
    jne 3f
    cmpb $102, 2(%r13)
    jne 3f
    call .Lputf
    add $3, %r13
    jmp 1b
7:  cmp $108, %al
    jne 3f
    inc %r13
    movzbq (%r13), %rax
    cmp $108, %al
    jne 3f
    inc %r13
    movzbq (%r13), %rax
3:  cmp $100, %al
    je 4f
    cmp $115, %al
    je 5f
    cmp $37, %al
    je 6f
    inc %r13
    jmp 1b
4:  call .Lputi
    inc %r13
    jmp 1b
5:  mov %r12, %rdi
    call .Lputs
    inc %r13
    jmp 1b
6:  mov $37, %eax
    call .Lputc
    inc %r13
    jmp 1b
9:  pop %r14
    pop %r13
    pop %r12
    xor %eax, %eax
    ret

.Lputc:
    sub $16, %rsp
    mov %al, (%rsp)
    mov $1, %eax
    mov $1, %edi
    mov %rsp, %rsi
    mov $1, %edx
    syscall
    add $16, %rsp
    ret

.Lputi:
    mov %r12, %rax
    xor %r10d, %r10d
    test %rax, %rax
    jns 1f
    neg %rax
    mov $1, %r10d
1:  lea .Lnumbuf+63(%rip), %rsi
    movb $0, (%rsi)
    mov $10, %rcx
2:  xor %rdx, %rdx
    div %rcx
    add $48, %dl
    dec %rsi
    mov %dl, (%rsi)
    test %rax, %rax
    jnz 2b
    test %r10d, %r10d
    jz 3f
    dec %rsi
    movb $45, (%rsi)
3:  lea .Lnumbuf+63(%rip), %rdx
    sub %rsi, %rdx
    mov $1, %eax
    mov $1, %edi
    syscall
    ret

.Lputs:
    push %rdi
    call strlen
    pop %rsi
    mov %rax, %rdx
    mov $1, %eax
    mov $1, %edi
    syscall
    ret

.section .text.Lir_panic_static
.globl .Lir_panic_static
.Lir_panic_static:
    mov %rdi, %rsi
    push %rsi
    call strlen
    mov %rax, %rdx
    pop %rsi
    mov $1, %eax
    mov $2, %edi
    syscall
    sub $16, %rsp
    movb $10, (%rsp)
    mov $1, %eax
    mov $2, %edi
    mov %rsp, %rsi
    mov $1, %edx
    syscall
    add $16, %rsp
    lea .Ltrace(%rip), %rsi
    mov $13, %edx
    mov $1, %eax
    mov $2, %edi
    syscall
    mov %rbp, %r12
1:  test %r12, %r12
    jz 2f
    mov 8(%r12), %rdi
    call .Lputhex
    mov (%r12), %r12
    jmp 1b
2:  mov $1, %edi
    mov $60, %eax
    syscall

.Lputhex:
    push %rbx
    lea .Lhbuf(%rip), %rbx
    mov $16, %ecx
    mov %rdi, %rax
3:  mov %al, %dl
    and $15, %dl
    cmp $10, %dl
    jb 4f
    add $87, %dl
    jmp 5f
4:  add $48, %dl
5:  dec %ecx
    mov %dl, (%rbx,%rcx)
    shr $4, %rax
    test %ecx, %ecx
    jnz 3b
    movb $10, 16(%rbx)
    mov $1, %eax
    mov $2, %edi
    mov %rbx, %rsi
    mov $17, %edx
    syscall
    pop %rbx
    ret

.section .text.sysconf
.globl sysconf
sysconf:
    mov $8, %eax
    ret

.section .text.pthread_create
.globl pthread_create
pthread_create:
    # rdi=tid*, rsi=attr, rdx=fn, rcx=arg
    push %rbx
    push %r12
    push %r13
    push %r14
    mov %rdi, %rbx
    mov %rdx, %r12
    mov %rcx, %r13
    # mmap 262144 bytes for the child stack
    xor %edi, %edi
    mov $262144, %esi
    mov $3, %edx
    mov $0x22, %r10d
    mov $-1, %r8
    xor %r9d, %r9d
    mov $9, %eax
    syscall
    cmp $-4095, %rax
    jae 9f
    lea 262144(%rax), %r14
    and $-16, %r14
    sub $16, %r14
    mov %r12, 0(%r14)
    mov %r13, 8(%r14)
    # clone(flags, stack, 0, 0, 0)
    mov $0xF11, %edi
    mov %r14, %rsi
    xor %edx, %edx
    xor %r10d, %r10d
    xor %r8d, %r8d
    mov $56, %eax
    syscall
    cmp $-4095, %rax
    jae 9f
    test %rax, %rax
    jz 1f
    mov %rax, (%rbx)
    xor %eax, %eax
    pop %r14
    pop %r13
    pop %r12
    pop %rbx
    ret
1:  # child: rsp is the child stack, trampoline reads fn/arg
    mov 0(%rsp), %rax
    mov 8(%rsp), %rdi
    call *%rax
    xor %edi, %edi
    mov $60, %eax
    syscall
9:  mov $11, %eax
    pop %r14
    pop %r13
    pop %r12
    pop %rbx
    ret

.section .text.pthread_join
.globl pthread_join
pthread_join:
    test %rdi, %rdi
    jz 1f
    xor %esi, %esi
    xor %edx, %edx
    xor %r10d, %r10d
    mov $61, %eax
    syscall
1:  xor %eax, %eax
    ret

.section .rodata
.Lperr: .ascii "static: print_prec precision must be <= 9\n"
.Lperr2: .ascii "static: float print overflow\n"

.section .bss
.p2align 4
.Lnbuf:
    .zero 64
.Lfbuf:
    .zero 160

.section .text.Lputf
.Lputf:
    # r14 = precision, xmm0 = value
    cmp $9, %r14d
    jbe 20f
    lea .Lperr(%rip), %rsi
    mov $42, %edx
    mov $1, %eax
    mov $1, %edi
    syscall
    mov $2, %edi
    mov $60, %eax
    syscall
20: sub $16, %rsp
    movq %xmm0, %rax
    mov %rax, (%rsp)
    fldl (%rsp)
    mov $1, %rax
    mov %r14d, %ecx
    test %ecx, %ecx
    jz 1f
0:  imul $10, %rax
    dec %ecx
    jnz 0b
1:  mov %rax, (%rsp)
    fildq (%rsp)
    fmulp %st, %st(1)
    fistpq (%rsp)
    mov (%rsp), %rax
    add $16, %rsp
    movabs $0x8000000000000000, %rcx
    cmp %rcx, %rax
    jne 21f
    lea .Lperr2(%rip), %rsi
    mov $33, %edx
    mov $1, %eax
    mov $1, %edi
    syscall
    mov $2, %edi
    mov $60, %eax
    syscall
21: xor %r11d, %r11d
    test %rax, %rax
    jns 2f
    neg %rax
    mov $1, %r11d
2:  lea .Lnbuf+63(%rip), %r8
    mov %r8, %rsi
    test %rax, %rax
    jnz 3f
    dec %rsi
    movb $48, (%rsi)
    jmp 4f
3:  mov $10, %rcx
5:  xor %rdx, %rdx
    div %rcx
    add $48, %dl
    dec %rsi
    mov %dl, (%rsi)
    test %rax, %rax
    jnz 5b
4:  mov %r8, %r9
    sub %rsi, %r9
    lea .Lfbuf(%rip), %rdi
    test %r11d, %r11d
    jz 6f
    movb $45, (%rdi)
    inc %rdi
6:  mov %r14, %r10
    cmp %r10, %r9
    ja 7f
    # 0.000ddd
    movb $48, (%rdi)
    inc %rdi
    movb $46, (%rdi)
    inc %rdi
    mov %r10, %rcx
    sub %r9, %rcx
    jz 9f
8:  movb $48, (%rdi)
    inc %rdi
    dec %rcx
    jnz 8b
9:  mov %r9, %rcx
    jmp 10f
7:  mov %r9, %rcx
    sub %r10, %rcx
11: mov (%rsi), %al
    mov %al, (%rdi)
    inc %rsi
    inc %rdi
    dec %rcx
    jnz 11b
    movb $46, (%rdi)
    inc %rdi
    mov %r10, %rcx
10: test %rcx, %rcx
    jz 12f
13: mov (%rsi), %al
    mov %al, (%rdi)
    inc %rsi
    inc %rdi
    dec %rcx
    jnz 13b
12: lea .Lfbuf(%rip), %rsi
    mov %rdi, %rdx
    sub %rsi, %rdx
    mov $1, %eax
    mov $1, %edi
    syscall
    ret

.section .text.fopen
.globl fopen
fopen:
    push %rbx
    push %r12
    mov %rdi, %rbx
    movzbl (%rsi), %eax
    mov $0, %esi
    mov $0644, %edx
    cmp $114, %al          # 'r'
    je 1f
    cmp $119, %al          # 'w'
    jne 2f
    mov $0x241, %esi
    jmp 1f
2:  cmp $97, %al           # 'a'
    jne 3f
    mov $0x441, %esi
    jmp 1f
3:  mov $-1, %rax
    jmp 4f
1:  mov %rbx, %rdi
    mov $2, %eax
    syscall
4:  pop %r12
    pop %rbx
    ret

.section .text.fclose
.globl fclose
fclose:
    test %rdi, %rdi
    js 1f
    mov $3, %eax
    syscall
    xor %eax, %eax
    ret
1:  mov $-1, %eax
    ret

.section .text.fread
.globl fread
fread:
    push %rbx
    push %r12
    mov %rsi, %rbx
    mov %rdx, %r12
    imul %rsi, %r12
    test %r12, %r12
    jz 1f
    mov %rdi, %rsi
    mov %rcx, %rdi
    mov %r12, %rdx
    xor %eax, %eax
    syscall
    xor %edx, %edx
    mov %rax, %rax
    test %rbx, %rbx
    jz 2f
    xor %edx, %edx
    div %rbx
    jmp 2f
1:  xor %eax, %eax
2:  pop %r12
    pop %rbx
    ret

.section .text.fwrite
.globl fwrite
fwrite:
    push %rbx
    push %r12
    mov %rsi, %rbx
    mov %rdx, %r12
    imul %rsi, %r12
    test %r12, %r12
    jz 1f
    mov %rdi, %rsi
    mov %rcx, %rdi
    mov %r12, %rdx
    mov $1, %eax
    syscall
    xor %edx, %edx
    test %rbx, %rbx
    jz 2f
    div %rbx
    jmp 2f
1:  xor %eax, %eax
2:  pop %r12
    pop %rbx
    ret

.section .text.fseek
.globl fseek
fseek:
    mov %rsi, %rsi
    mov %edx, %edx
    mov $8, %eax
    syscall
    test %rax, %rax
    js 1f
    xor %eax, %eax
    ret
1:  mov $-1, %eax
    ret

.section .text.ftell
.globl ftell
ftell:
    xor %esi, %esi
    mov $1, %edx
    mov $8, %eax
    syscall
    ret

.section .text.fflush
.globl fflush
fflush:
    xor %eax, %eax
    ret

.section .text.read
.globl read
read:
    xor %eax, %eax
    syscall
    ret

.section .text.write
.globl write
write:
    mov $1, %eax
    syscall
    ret

.section .text.close
.globl close
close:
    mov $3, %eax
    syscall
    ret

.section .text.socket
.globl socket
socket:
    mov $41, %eax
    syscall
    ret

.section .text.bind
.globl bind
bind:
    mov $49, %eax
    syscall
    ret

.section .text.listen
.globl listen
listen:
    mov $50, %eax
    syscall
    ret

.section .text.connect
.globl connect
connect:
    mov $42, %eax
    syscall
    ret

.section .text.getsockname
.globl getsockname
getsockname:
    mov $51, %eax
    syscall
    ret

.section .text.accept
.globl accept
accept:
    mov $43, %eax
    syscall
    ret

.section .text.inet_addr
.globl inet_addr
inet_addr:
    # rdi = "a.b.c.d" -> u32 whose little-endian bytes are a,b,c,d
    xor %r8d, %r8d
    xor %r9d, %r9d
    xor %ecx, %ecx
1:  movzbl (%rdi), %edx
    inc %rdi
    cmp $46, %dl
    je 2f
    test %dl, %dl
    jz 2f
    sub $48, %edx
    imul $10, %ecx, %ecx
    add %edx, %ecx
    jmp 1b
2:  mov %ecx, %edx
    mov %r9d, %ecx
    shl $3, %ecx
    shl %cl, %edx
    or %edx, %r8d
    inc %r9d
    xor %ecx, %ecx
    cmpb $0, -1(%rdi)
    jne 1b
    mov %r8d, %eax
    ret

.section .text.strcmp
.globl strcmp
strcmp:
1:  movzbl (%rdi), %eax
    movzbl (%rsi), %edx
    cmp %edx, %eax
    jne 2f
    test %eax, %eax
    jz 3f
    inc %rdi
    inc %rsi
    jmp 1b
2:  sub %edx, %eax
    ret
3:  xor %eax, %eax
    ret

.section .rodata
.Ltrace: .ascii "stack trace:\n"

.section .bss
.p2align 4
.Lhbuf:
    .zero 32
.p2align 3
.Lenvp:
    .zero 8

.section .text.getenv
.globl getenv
getenv:
    mov .Lenvp(%rip), %r8
    test %r8, %r8
    jz 9f
1:  mov (%r8), %rsi
    test %rsi, %rsi
    jz 9f
    mov %rdi, %r9
    mov %rsi, %r10
2:  movzbl (%r9), %eax
    test %al, %al
    jz 3f
    movzbl (%r10), %edx
    cmp %dl, %al
    jne 4f
    inc %r9
    inc %r10
    jmp 2b
3:  cmpb $61, (%r10)
    jne 4f
    lea 1(%r10), %rax
    ret
4:  add $8, %r8
    jmp 1b
9:  xor %eax, %eax
    ret

.section .text.malloc
.globl malloc
malloc:
    push %rbx
    mov %rdi, %rbx
    lea 16(%rbx), %rsi
    add $4095, %rsi
    and $-4096, %rsi
    xor %edi, %edi
    mov $3, %edx
    mov $0x22, %r10d
    mov $-1, %r8
    xor %r9d, %r9d
    mov $9, %eax
    syscall
    cmp $-4095, %rax
    jae 9f
    lea 16(%rax), %rdx
    mov %rbx, -8(%rdx)
    mov %rdx, %rax
    pop %rbx
    ret
9:  xor %eax, %eax
    pop %rbx
    ret

.section .text.realloc
.globl realloc
realloc:
    push %rbx
    push %r12
    push %r13
    push %r14
    sub $8, %rsp
    mov %rdi, %r12
    mov %rsi, %r13
    test %r12, %r12
    jz 8f
    test %r13, %r13
    jz 7f
    mov %r13, %rdi
    call malloc
    test %rax, %rax
    jz 7f
    mov %rax, %r14
    mov -8(%r12), %rbx
    cmp %r13, %rbx
    jbe 1f
    mov %r13, %rbx
1:  test %rbx, %rbx
    jz 3f
    xor %ecx, %ecx
2:  mov (%r12,%rcx), %dl
    mov %dl, (%r14,%rcx)
    inc %rcx
    cmp %rbx, %rcx
    jb 2b
3:  mov %r14, %rax
    jmp 9f
7:  xor %eax, %eax
    jmp 9f
8:  mov %r13, %rdi
    call malloc
9:  add $8, %rsp
    pop %r14
    pop %r13
    pop %r12
    pop %rbx
    ret
