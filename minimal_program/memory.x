MEMORY
{
    FLASH :     ORIGIN = 0x08000000, LENGTH = 128k
    RAM :       ORIGIN = 0x20000000, LENGTH = 40K
}

SECTIONS {
  .crapsection (INFO) :
  {
    *(.crapsection .crapsection.*);
  }
}