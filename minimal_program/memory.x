MEMORY
{
    FLASH :     ORIGIN = 0x08000000, LENGTH = 128k
    RAM :       ORIGIN = 0x20000000, LENGTH = 39K
    PANDUMP:    ORIGIN = 0x20009C00, LENGTH = 1K
}

/* Provided addresses */
/* PROVIDE(_panic_dump_start = ORIGIN(PANDUMP)); */
/* PROVIDE(_panic_dump_end   = ORIGIN(PANDUMP) + LENGTH(PANDUMP)); */
