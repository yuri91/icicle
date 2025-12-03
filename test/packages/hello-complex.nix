{ stdenv, gcc, writeText }:

stdenv.mkDerivation {
  pname = "hello-complex";
  version = "1.0.0";
  src = writeText "hello.c" ''
    #include <stdio.h>
    #include <time.h>
    int main() {
      time_t now = time(0);
      printf("Hello from complex C package!\n");
      printf("Compiled at: %s", ctime(&now));
      return 0;
    }
  '';
  buildInputs = [ gcc ];
  phases = [ "buildPhase" "installPhase" ];
  buildPhase = ''
    gcc -o hello $src
  '';
  installPhase = ''
    mkdir -p $out/bin
    cp hello $out/bin/
  '';
}
