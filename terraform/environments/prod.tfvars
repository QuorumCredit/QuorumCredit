environment = "prod"
aws_region  = "us-east-1"

# VPC Configuration
vpc_cidr                = "10.0.0.0/16"
availability_zones      = ["us-east-1a", "us-east-1b", "us-east-1c"]
private_subnet_cidrs    = ["10.0.1.0/24", "10.0.2.0/24", "10.0.3.0/24"]
public_subnet_cidrs     = ["10.0.101.0/24", "10.0.102.0/24", "10.0.103.0/24"]
database_subnet_cidrs   = ["10.0.201.0/24", "10.0.202.0/24", "10.0.203.0/24"]
enable_nat_gateway      = true
single_nat_gateway      = false  # HA across all AZs

# EKS Configuration
kubernetes_version = "1.28"
eks_desired_size   = 3
eks_min_size       = 3
eks_max_size       = 20
eks_instance_types = ["m5.xlarge", "m5.2xlarge"]
eks_disk_size      = 100

# RDS Configuration (password from environment variable)
db_instance_class        = "db.r5.xlarge"
db_allocated_storage     = 500
db_max_allocated_storage = 2000

# Monitoring
enable_monitoring = true

# CDN
enable_cdn      = true
domain_name     = "quorumcredit.xyz"
acm_certificate_arn = "arn:aws:acm:us-east-1:123456789:certificate/xxxxx"  # Update with real ARN

# Alarms
alarm_email_recipients = [
  "devops@quorumcredit.io",
  "vp-eng@quorumcredit.io"
]

# Cost Center
cost_center = "engineering-prod"
