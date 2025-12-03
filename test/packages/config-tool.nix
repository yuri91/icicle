{ writeShellScriptBin, jq, hello-simple }:

writeShellScriptBin "config-tool" ''
  PATH=${jq}/bin:${hello-simple}/bin:$PATH
  
  echo "Config tool v1.0 (depends on hello-simple)"
  echo "Available commands: init, validate, deploy, test"
  
  case "''${1:-help}" in
    init) 
      echo "Initializing configuration..."
      echo '{"version": "1.0", "initialized": true}' | jq . > config.json
      echo "Created config.json"
      ;;
    validate) 
      echo "Validating configuration..." 
      if [ -f config.json ]; then
        jq . config.json > /dev/null && echo "Valid JSON configuration"
      else
        echo "No config.json found"
        exit 1
      fi
      ;;
    deploy) 
      echo "Deploying configuration..."
      echo "Testing dependency first:"
      hello-simple
      echo "Deployment would happen here"
      ;;
    test)
      echo "Testing dependencies:"
      hello-simple
      ;;
    *) 
      echo "Usage: config-tool [init|validate|deploy|test]"
      exit 1
      ;;
  esac
''
