#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <elf.h>

// Clears DF_1_PIE (0x08000000) in DT_FLAGS_1 (0x6ffffffb) of a 64-bit ELF PIE binary
// so that glibc's dlopen() (Node.js require()) and Linux's execve() BOTH accept the exact same file!
int main(int argc, char** argv) {
    if (argc < 2) return 1;
    FILE* f = fopen(argv[1], "r+b");
    if (!f) { perror("fopen"); return 1; }

    Elf64_Ehdr ehdr;
    if (fread(&ehdr, sizeof(ehdr), 1, f) != 1) return 1;

    for (int i = 0; i < ehdr.e_phnum; i++) {
        Elf64_Phdr phdr;
        fseek(f, ehdr.e_phoff + i * ehdr.e_phentsize, SEEK_SET);
        if (fread(&phdr, sizeof(phdr), 1, f) != 1) return 1;

        if (phdr.p_type == PT_DYNAMIC) {
            size_t count = phdr.p_filesz / sizeof(Elf64_Dyn);
            for (size_t j = 0; j < count; j++) {
                Elf64_Dyn dyn;
                long pos = phdr.p_offset + j * sizeof(Elf64_Dyn);
                fseek(f, pos, SEEK_SET);
                if (fread(&dyn, sizeof(dyn), 1, f) != 1) break;
                if (dyn.d_tag == DT_NULL) break;
                if (dyn.d_tag == DT_FLAGS_1) {
                    dyn.d_un.d_val &= ~((Elf64_Xword)DF_1_PIE);
                    fseek(f, pos, SEEK_SET);
                    fwrite(&dyn, sizeof(dyn), 1, f);
                }
            }
        }
    }
    fclose(f);
    return 0;
}
