ENTRY(_start)

/* Định nghĩa các loại Segment */
PHDRS
{
    text PT_LOAD FLAGS(5);    /* R-E (Read + Execute) */
    rodata PT_LOAD FLAGS(4);  /* R-- (Read Only) */
    data PT_LOAD FLAGS(6);    /* RW- (Read + Write) */
}

SECTIONS {
    . = 0xffffffff80000000;

    .text : ALIGN(4K) {
        *(.text .text.*)
    } :text

    /* Ép nhảy sang trang mới và tạo Segment mới */
    . = ALIGN(4K);
    .rodata : ALIGN(4K) {
        *(.rodata .rodata.*)
    } :rodata

    . = ALIGN(4K);
    .data : ALIGN(4K) {
        *(.data .data.*)
        *(.bss .bss.*)
        *(COMMON)
    } :data
}