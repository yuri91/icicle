{ writeShellScriptBin, hello-simple, config-tool, data-processor }:

writeShellScriptBin "full-suite" ''
  echo "Full test suite v1.0"
  echo "This package depends on: hello-simple, config-tool, data-processor"
  echo ""
  
  echo "=== Testing hello-simple ==="
  ${hello-simple}/bin/hello-simple
  echo ""
  
  echo "=== Testing config-tool ==="
  ${config-tool}/bin/config-tool test
  echo ""
  
  echo "=== Testing data-processor ==="
  echo "data-processor dependencies:"
  cat ${data-processor}/dependencies.txt
  echo ""
  
  echo "=== All tests completed ==="
  echo "Dependencies verified successfully"
''
