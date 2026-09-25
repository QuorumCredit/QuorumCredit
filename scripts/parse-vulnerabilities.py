#!/usr/bin/env python3
"""
Parse Trivy vulnerability scan results and generate human-readable report.
"""

import json
import sys
from collections import defaultdict
from datetime import datetime


def parse_vulnerabilities(json_file):
    """Parse Trivy JSON output and generate report."""

    try:
        with open(json_file, 'r') as f:
            data = json.load(f)
    except (FileNotFoundError, json.JSONDecodeError) as e:
        print(f"Error reading file: {e}")
        return

    # Initialize counters
    vulnerabilities = defaultdict(lambda: defaultdict(list))
    misconfigurations = defaultdict(list)
    secrets = []

    # Parse results
    if 'Results' in data:
        for result in data['Results']:
            # Process vulnerabilities
            if 'Vulnerabilities' in result:
                for vuln in result['Vulnerabilities']:
                    severity = vuln.get('Severity', 'UNKNOWN')
                    vulnerabilities[severity].append({
                        'id': vuln.get('VulnerabilityID', 'N/A'),
                        'package': result.get('Target', 'unknown'),
                        'title': vuln.get('Title', 'N/A'),
                        'score': vuln.get('CVSS', {}).get('nvd', {}).get('V3Score', 'N/A'),
                        'fixed_in': vuln.get('FixedVersion', 'N/A')
                    })

            # Process misconfigurations
            if 'Misconfigurations' in result:
                for config in result['Misconfigurations']:
                    misconfigurations[config.get('Severity', 'UNKNOWN')].append({
                        'title': config.get('Title', 'N/A'),
                        'description': config.get('Description', 'N/A'),
                        'id': config.get('ID', 'N/A'),
                    })

            # Process secrets
            if 'Secrets' in result:
                for secret in result['Secrets']:
                    secrets.append({
                        'type': secret.get('Title', 'N/A'),
                        'location': secret.get('StartLine', 'unknown'),
                        'rule': secret.get('RuleID', 'N/A'),
                    })

    # Generate report
    print("=" * 80)
    print("CONTAINER IMAGE VULNERABILITY SCAN REPORT")
    print("=" * 80)
    print(f"Generated: {datetime.now().isoformat()}")
    print(f"Source: {json_file}")
    print()

    # Vulnerability summary
    print("VULNERABILITY SUMMARY")
    print("-" * 80)
    severity_order = ['CRITICAL', 'HIGH', 'MEDIUM', 'LOW', 'UNKNOWN']
    total_vulns = 0

    for severity in severity_order:
        count = len(vulnerabilities[severity])
        if count > 0:
            total_vulns += count
            emoji = {
                'CRITICAL': '🔴',
                'HIGH': '🟠',
                'MEDIUM': '🟡',
                'LOW': '🔵',
                'UNKNOWN': '⚪'
            }.get(severity, '❓')
            print(f"{emoji} {severity:8s}: {count:3d} vulnerabilities")

    print(f"\nTotal: {total_vulns} vulnerabilities")
    print()

    # Detailed vulnerabilities
    if total_vulns > 0:
        print("VULNERABILITIES DETAIL")
        print("-" * 80)

        for severity in severity_order:
            vulns = vulnerabilities[severity]
            if not vulns:
                continue

            print(f"\n{severity} ({len(vulns)} found):")
            print()

            for vuln in sorted(vulns, key=lambda x: str(x['score']), reverse=True):
                print(f"  ID: {vuln['id']}")
                print(f"  Package: {vuln['package']}")
                print(f"  Title: {vuln['title']}")
                print(f"  CVSS Score: {vuln['score']}")
                print(f"  Fixed In: {vuln['fixed_in']}")
                print()

    # Misconfigurations
    if any(misconfigurations.values()):
        print("\nMISCONFIGURATIONS")
        print("-" * 80)
        total_configs = 0

        for severity in severity_order:
            configs = misconfigurations[severity]
            if configs:
                total_configs += len(configs)
                emoji = {
                    'CRITICAL': '🔴',
                    'HIGH': '🟠',
                    'MEDIUM': '🟡',
                    'LOW': '🔵',
                    'UNKNOWN': '⚪'
                }.get(severity, '❓')
                print(f"{emoji} {severity:8s}: {len(configs):3d} misconfigurations")

        print(f"\nTotal: {total_configs} misconfigurations")

    # Secrets
    if secrets:
        print("\nSECRETS DETECTED")
        print("-" * 80)
        print(f"⚠️  Found {len(secrets)} potential secrets:\n")

        for secret in secrets:
            print(f"  Type: {secret['type']}")
            print(f"  Rule: {secret['rule']}")
            print(f"  Location: Line {secret['location']}")
            print()

    # Recommendations
    print("\nRECOMMENDATIONS")
    print("-" * 80)

    if vulnerabilities['CRITICAL']:
        print("🔴 CRITICAL: Immediately patch these vulnerabilities before deploying.")
        print()

    if vulnerabilities['HIGH']:
        print("🟠 HIGH: Patch these vulnerabilities as soon as possible.")
        print()

    if vulnerabilities['MEDIUM']:
        print("🟡 MEDIUM: Plan to patch these in the next release cycle.")
        print()

    if secrets:
        print("⚠️  SECRETS: Remove exposed credentials and rotate them immediately.")
        print()

    if any(misconfigurations.values()):
        print("⚙️  CONFIG: Review and fix security misconfigurations.")
        print()

    if total_vulns == 0 and not secrets and not any(misconfigurations.values()):
        print("✅ No vulnerabilities, secrets, or misconfigurations detected!")

    print("\n" + "=" * 80)

    # Return exit code based on critical vulnerabilities
    return 1 if vulnerabilities['CRITICAL'] else 0


if __name__ == '__main__':
    if len(sys.argv) < 2:
        print("Usage: parse-vulnerabilities.py <trivy-json-file>")
        sys.exit(1)

    exit_code = parse_vulnerabilities(sys.argv[1])
    sys.exit(exit_code)
