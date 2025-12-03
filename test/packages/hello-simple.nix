{ writeShellScriptBin }:

writeShellScriptBin "hello-simple" ''
  echo "Hello from simple package!"
  echo "Current time: $(date)"
  echo "System: $(uname -a)"
''
