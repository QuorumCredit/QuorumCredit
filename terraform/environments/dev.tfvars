environment = "dev"
aws_region  = "us-east-1"

# VPC Configuration
vpc_cidr                = "10.0.0.0/16"
availability_zones      = ["us-east-1a", "us-east-1b"]
private_subnet_cidrs    = ["10.0.1.0/24", "10.0.2.0/24"]
public_subnet_cidrs     = ["10.0.101.0/24", "10.0.102.0/24"]
database_subnet_cidrs   = ["10.0.201.0/24", "10.0.202.0/24"]
enable_nat_gateway      = true
single_nat_gateway      = true

# EKS Configuration
kubernetes_version = "1.28"
eks_desired_size   = 2
eks_min_size       = 1
eks_max_size       = 5
eks_instance_types = ["t3.large"]
eks_disk_size      = 50

# RDS Configuration (password from environment variable)
db_instance_class      = "db.t3.small"
db_allocated_storage   = 50
db_max_allocated_storage = 100

# Monitoring
enable_monitoring = true

# Cost Center
cost_center = "engineering-dev"
