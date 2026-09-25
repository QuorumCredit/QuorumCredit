# QuorumCredit Terraform Configuration

Infrastructure as Code (IaC) for QuorumCredit using Terraform and AWS.

## Quick Start

### Initialize Terraform

```bash
cd terraform
terraform init
```

### Plan Infrastructure

```bash
# For development
terraform plan -var-file=environments/dev.tfvars

# For production
terraform plan -var-file=environments/prod.tfvars
```

### Apply Changes

```bash
# For development
terraform apply -var-file=environments/dev.tfvars

# For production
terraform apply -var-file=environments/prod.tfvars
```

## Directory Structure

```
terraform/
├── modules/           # Reusable Terraform modules
├── environments/      # Environment-specific configurations
├── main.tf           # Root module configuration
├── variables.tf      # Input variable definitions
├── outputs.tf        # Output definitions
├── backend.tf        # Backend configuration (documentation)
└── README.md         # This file
```

## Environments

| Environment | Purpose | Nodes | Database | Backup |
|------------|---------|-------|----------|--------|
| dev | Development/testing | 1-2 | t3.small | 7 days |
| staging | Pre-production | 2-3 | t3.large | 30 days |
| prod | Production | 3+ | r5.xlarge | 30 days |

## Common Commands

### Get Cluster Information

```bash
# Get cluster name
terraform output eks_cluster_name

# Get database endpoint
terraform output rds_endpoint

# Get VPC ID
terraform output vpc_id
```

### Update Configuration

```bash
# Scale nodes
terraform apply -var-file=environments/prod.tfvars \
  -var="eks_desired_size=5"

# Update Kubernetes version
terraform apply -var-file=environments/prod.tfvars \
  -var="kubernetes_version=1.29"

# Increase database storage
terraform apply -var-file=environments/prod.tfvars \
  -var="db_allocated_storage=1000"
```

### View Current State

```bash
# List all resources
terraform state list

# Show specific resource
terraform state show aws_eks_cluster.main

# Validate configuration
terraform validate

# Format code
terraform fmt -recursive
```

## State Management

State is stored in S3 with DynamoDB locking:

- **Bucket:** quorum-credit-terraform-state
- **Lock Table:** quorum-credit-tf-locks
- **Region:** us-east-1

View state:
```bash
aws s3 ls s3://quorum-credit-terraform-state/
aws dynamodb scan --table-name quorum-credit-tf-locks
```

## Secrets Management

Database password and other secrets should be provided via environment variables:

```bash
export TF_VAR_db_password="<secure-password>"
export TF_VAR_grafana_admin_password="<secure-password>"

terraform apply -var-file=environments/prod.tfvars
```

Or use AWS Secrets Manager:
```bash
export TF_VAR_db_password=$(aws secretsmanager get-secret-value \
  --secret-id quorum-credit/db/password \
  --query SecretString --output text)
```

## Modules

### VPC Module
Network infrastructure including subnets, route tables, and NAT gateways.

**Key Resources:**
- VPC
- Public/Private/Database subnets
- Internet Gateway
- NAT Gateways
- Route tables

### EKS Module
Kubernetes cluster infrastructure.

**Key Resources:**
- EKS Cluster
- Node Groups
- IAM Roles/Policies
- Security Groups
- Cluster Addons

### RDS Module
PostgreSQL database for application data.

**Key Resources:**
- RDS Instance
- Database subnet group
- Security group
- Parameter group
- Enhanced monitoring

### S3 Module
Cloud storage for logs, backups, and artifacts.

**Key Resources:**
- S3 Buckets
- Bucket policies
- Versioning configuration
- Lifecycle rules
- Encryption

### CloudFront Module (Optional)
CDN for static assets.

**Key Resources:**
- Distribution
- Origin configuration
- Cache behaviors
- SSL certificate

### Monitoring Module
CloudWatch, Prometheus, and Grafana.

**Key Resources:**
- CloudWatch Log Groups
- Alarms
- SNS Topics
- Prometheus Stack
- Grafana Deployment

## Deployment Workflow

1. **Create PR with Changes**
   ```bash
   git checkout -b terraform/feature-name
   # Make changes
   terraform plan -var-file=environments/prod.tfvars > plan.txt
   git add terraform/
   git commit -m "terraform: description of changes"
   git push origin terraform/feature-name
   gh pr create
   ```

2. **Review and Approve**
   - Code review of Terraform changes
   - Verify plan output (plan.txt)
   - Check for security implications

3. **Merge and Deploy**
   - Merge PR to main branch
   - GitHub Actions runs terraform apply
   - Monitor deployment

4. **Verify**
   - Check cluster health: `kubectl get nodes`
   - Verify services: `kubectl get pods -A`
   - Check database: `psql $DB_ENDPOINT -c "SELECT 1;"`

## Monitoring

### EKS Cluster Health

```bash
# Get cluster info
kubectl cluster-info

# Check nodes
kubectl get nodes -o wide

# Check pod status
kubectl get pods -A

# View events
kubectl get events -A --sort-by='.lastTimestamp'
```

### Database Health

```bash
# Connect to database
psql postgresql://admin@$RDS_ENDPOINT/quorum

# Check connections
SELECT count(*) FROM pg_stat_activity;

# Check table sizes
SELECT schemaname, tablename, 
       pg_size_pretty(pg_total_relation_size(schemaname||'.'||tablename)) AS size
FROM pg_tables 
ORDER BY pg_total_relation_size(schemaname||'.'||tablename) DESC;
```

## Troubleshooting

### State Lock Issues

```bash
# View current locks
aws dynamodb scan --table-name quorum-credit-tf-locks

# Force unlock (use with caution!)
terraform force-unlock <LOCK_ID>
```

### API Rate Limiting

```bash
# Retry with backoff
terraform apply -lock-timeout=5m -var-file=environments/prod.tfvars

# Reduce parallelism
terraform apply -parallelism=1 -var-file=environments/prod.tfvars
```

### State Refresh

```bash
# Refresh state from AWS
terraform refresh -var-file=environments/prod.tfvars

# Show differences
terraform plan -var-file=environments/prod.tfvars
```

## Security

- State files encrypted at rest in S3
- DynamoDB locking prevents concurrent modifications
- IAM roles follow principle of least privilege
- Network traffic encrypted in transit
- Database encryption enabled
- Secrets stored in AWS Secrets Manager

## Cost Management

- Development: Single NAT Gateway (cost optimized)
- Staging: Multiple NAT Gateways (HA at higher cost)
- Production: Reserved Instances + Spot for compute
- Auto-scaling enabled for flexibility

## Contributing

1. Create a feature branch
2. Make Terraform changes
3. Run `terraform fmt -recursive` for formatting
4. Run `terraform validate` for validation
5. Plan changes: `terraform plan`
6. Commit with clear message
7. Create PR with plan output

## Getting Help

- [Terraform Docs](https://www.terraform.io/docs)
- [AWS Provider Docs](https://registry.terraform.io/providers/hashicorp/aws/latest/docs)
- [EKS Best Practices](https://aws.amazon.com/eks/best-practices/)
- Team Slack: #infrastructure

## References

- [INFRASTRUCTURE_AS_CODE.md](../docs/INFRASTRUCTURE_AS_CODE.md) - Detailed documentation
- [DISASTER_RECOVERY.md](../docs/DISASTER_RECOVERY.md) - DR procedures
