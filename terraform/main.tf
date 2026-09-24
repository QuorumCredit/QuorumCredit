terraform {
  required_version = ">= 1.5.0"

  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 5.0"
    }
    kubernetes = {
      source  = "hashicorp/kubernetes"
      version = "~> 2.23"
    }
    helm = {
      source  = "hashicorp/helm"
      version = "~> 2.11"
    }
  }

  backend "s3" {
    bucket         = "quorum-credit-terraform-state"
    key            = "prod/terraform.tfstate"
    region         = "us-east-1"
    encrypt        = true
    dynamodb_table = "quorum-credit-tf-locks"
  }
}

provider "aws" {
  region = var.aws_region

  default_tags {
    tags = {
      Project        = "QuorumCredit"
      Environment    = var.environment
      ManagedBy      = "Terraform"
      CreatedAt      = timestamp()
      CostCenter     = var.cost_center
    }
  }
}

provider "kubernetes" {
  host                   = data.aws_eks_cluster.cluster.endpoint
  cluster_ca_certificate = base64decode(data.aws_eks_cluster.cluster.certificate_authority[0].data)
  token                  = data.aws_eks_cluster_auth.cluster.token
}

provider "helm" {
  kubernetes {
    host                   = data.aws_eks_cluster.cluster.endpoint
    cluster_ca_certificate = base64decode(data.aws_eks_cluster.cluster.certificate_authority[0].data)
    token                  = data.aws_eks_cluster_auth.cluster.token
  }
}

# Data sources for EKS cluster
data "aws_eks_cluster" "cluster" {
  name = module.eks.cluster_name
}

data "aws_eks_cluster_auth" "cluster" {
  name = module.eks.cluster_name
}

# VPC Module
module "vpc" {
  source = "./modules/vpc"

  environment              = var.environment
  vpc_cidr                 = var.vpc_cidr
  availability_zones       = var.availability_zones
  private_subnet_cidrs     = var.private_subnet_cidrs
  public_subnet_cidrs      = var.public_subnet_cidrs
  database_subnet_cidrs    = var.database_subnet_cidrs
  enable_nat_gateway       = var.enable_nat_gateway
  single_nat_gateway       = var.single_nat_gateway
  enable_dns_hostnames     = true
  enable_dns_support       = true

  tags = {
    Name = "quorum-credit-vpc-${var.environment}"
  }
}

# EKS Module
module "eks" {
  source = "./modules/eks"

  environment              = var.environment
  cluster_name             = "quorum-credit-${var.environment}"
  cluster_version          = var.kubernetes_version
  vpc_id                   = module.vpc.vpc_id
  subnet_ids               = concat(module.vpc.private_subnet_ids, module.vpc.public_subnet_ids)

  node_groups = {
    general = {
      name           = "general"
      capacity_type  = "ON_DEMAND"
      desired_size   = var.eks_desired_size
      min_size       = var.eks_min_size
      max_size       = var.eks_max_size
      instance_types = var.eks_instance_types
      disk_size      = var.eks_disk_size

      labels = {
        workload = "general"
      }

      tags = {
        NodeGroup = "general"
      }
    }

    compute = {
      name           = "compute"
      capacity_type  = "SPOT"
      desired_size   = 2
      min_size       = 1
      max_size       = 10
      instance_types = ["c5.2xlarge", "c6i.2xlarge"]
      disk_size      = 100

      labels = {
        workload = "compute"
      }

      taints = [{
        key    = "workload"
        value  = "compute"
        effect = "NoSchedule"
      }]
    }
  }

  cluster_enabled_log_types = ["api", "audit", "authenticator", "controllerManager", "scheduler"]
  cluster_log_retention_in_days = 30

  tags = {
    ClusterName = "quorum-credit-${var.environment}"
  }
}

# RDS Database Module
module "rds" {
  source = "./modules/rds"

  environment                = var.environment
  engine                     = "postgres"
  engine_version             = "15.4"
  db_name                    = "quorum"
  username                   = "admin"
  password                   = var.db_password  # From environment/secrets

  instance_class             = var.db_instance_class
  allocated_storage           = var.db_allocated_storage
  max_allocated_storage       = var.db_max_allocated_storage
  storage_encrypted           = true
  storage_type               = "gp3"
  iops                       = 3000

  multi_az                   = true
  backup_retention_period    = 30
  backup_window              = "03:00-04:00"
  maintenance_window         = "sun:04:00-sun:05:00"

  vpc_id                     = module.vpc.vpc_id
  db_subnet_ids              = module.vpc.database_subnet_ids
  allowed_security_groups    = [module.eks.worker_security_group_id]

  enable_cloudwatch_logs     = true
  log_exports                = ["postgresql"]

  enable_performance_insights = true
  performance_insights_retention_period = 31

  tags = {
    Database = "quorum-credit"
  }
}

# S3 Buckets Module
module "s3_buckets" {
  source = "./modules/s3"

  environment = var.environment

  buckets = {
    logs = {
      name    = "quorum-credit-logs-${data.aws_caller_identity.current.account_id}"
      purpose = "Application and access logs"
      versioning = true
      retention_days = 90
    }

    backups = {
      name    = "quorum-credit-backups-${data.aws_caller_identity.current.account_id}"
      purpose = "Database and application backups"
      versioning = true
      retention_days = 365
      replication_region = "us-west-2"
    }

    artifacts = {
      name    = "quorum-credit-artifacts-${data.aws_caller_identity.current.account_id}"
      purpose = "Build artifacts and releases"
      versioning = true
      retention_days = 180
    }
  }

  tags = {
    S3Purpose = "Data storage"
  }
}

# CloudFront Distribution Module (if needed for static assets)
module "cloudfront" {
  source = "./modules/cloudfront"

  count = var.enable_cdn ? 1 : 0

  environment          = var.environment
  domain_name          = var.domain_name
  certificate_arn      = var.acm_certificate_arn
  bucket_regional_name = module.s3_buckets.artifacts_bucket_regional_domain_name

  tags = {
    Service = "CDN"
  }
}

# Monitoring Module
module "monitoring" {
  source = "./modules/monitoring"

  environment                  = var.environment
  cluster_name                 = module.eks.cluster_name
  enable_prometheus            = var.enable_monitoring
  enable_grafana               = var.enable_monitoring
  grafana_admin_password       = var.grafana_admin_password

  alarm_email_recipients       = var.alarm_email_recipients
  alarm_slack_webhook_url      = var.alarm_slack_webhook_url

  tags = {
    Monitoring = "enabled"
  }
}

# Data source to get current AWS account ID
data "aws_caller_identity" "current" {}

# Output key infrastructure details
output "eks_cluster_name" {
  description = "EKS Cluster name"
  value       = module.eks.cluster_name
}

output "eks_cluster_endpoint" {
  description = "EKS Cluster endpoint"
  value       = module.eks.cluster_endpoint
}

output "rds_endpoint" {
  description = "RDS Database endpoint"
  value       = module.rds.db_instance_endpoint
  sensitive   = true
}

output "rds_database_name" {
  description = "RDS Database name"
  value       = module.rds.db_instance_name
}

output "vpc_id" {
  description = "VPC ID"
  value       = module.vpc.vpc_id
}

output "vpc_cidr" {
  description = "VPC CIDR block"
  value       = module.vpc.vpc_cidr
}

output "s3_log_bucket" {
  description = "S3 bucket for logs"
  value       = module.s3_buckets.logs_bucket_name
}

output "s3_backup_bucket" {
  description = "S3 bucket for backups"
  value       = module.s3_buckets.backups_bucket_name
}
