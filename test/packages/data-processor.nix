{ python3Packages, writeText, hello-complex }:

python3Packages.buildPythonApplication {
  pname = "data-processor";
  version = "0.1.0";
  src = writeText "processor.py" ''
    #!/usr/bin/env python3
    import json
    import sys
    import time
    
    def process_data():
        print("Processing data...")
        time.sleep(1)  # Simulate work
        data = {
          "processed": True, 
          "timestamp": time.strftime("%Y-%m-%d %H:%M:%S"),
          "pid": os.getpid() if 'os' in globals() else 0
        }
        print(json.dumps(data, indent=2))
    
    if __name__ == "__main__":
        import os
        process_data()
  '';
  format = "other";
  buildInputs = [ hello-complex ];
  installPhase = ''
    mkdir -p $out/bin
    cp $src $out/bin/data-processor
    chmod +x $out/bin/data-processor
    
    # Reference hello-complex to create dependency
    echo "Depends on: ${hello-complex}" > $out/dependencies.txt
  '';
  propagatedBuildInputs = with python3Packages; [ requests ];
}
