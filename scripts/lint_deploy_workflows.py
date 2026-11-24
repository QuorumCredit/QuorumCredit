#!/usr/bin/env python3
"""
Lint GitHub deploy workflows to ensure they have environment protection.
Fails if any deployment workflow lacks an environment: block.
"""
import sys
import glob
import re

def main():
    deploy_workflows = glob.glob(".github/workflows/*deploy*.yml")
    failed = False
    
    for wf in deploy_workflows:
        with open(wf, "r", encoding="utf-8") as f:
            content = f.read()
        
        # Check if environment: is defined in any job
        if not re.search(r"^\s*environment:\s*", content, re.MULTILINE):
            print(f"ERROR: {wf} lacks an `environment:` block for reviewer protection.", file=sys.stderr)
            failed = True
        else:
            print(f"OK: {wf} has an `environment:` block.")

    if failed:
        sys.exit(1)
    print("All deploy workflows passed environment protection check.")

if __name__ == "__main__":
    main()
