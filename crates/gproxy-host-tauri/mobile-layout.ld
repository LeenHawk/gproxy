/* UPX keeps the shared-library prefix before the first executable section.
 * Move ordinary constants behind the code, keeping loader/unwind metadata
 * in the linker's default positions. */
SECTIONS {
  .rodata : { *(.rodata .rodata.*) }
} INSERT AFTER .plt;
