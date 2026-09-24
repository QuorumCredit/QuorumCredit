variable "aws_region" {
  description = "AWS region"
  type        = string
  default     = "us-east-1"
}

variable "environment" {
  description = "Environment name (dev, staging, prod)"
  type        = string
  validation {
    condition     = contains(["dev", "staging", "prod"], var.environment)
    error_message = "Environment must be dev, staging, or prod."
  }
}

variable "cost_center" {
  description = "Cost center for billing allocation"
  type        = string
  default     = "engineering"
}

# VPC Configuration
variable "vpc_cidr" {
  description = "CIDR block for VPC"
  type        = string
  default     = "10.0.0.0/16"
}

variable "availability_zones" {
  description = "Availability zones for the region"
  type        = list(string)
  default     = ["us-east-1a", "us-east-1b", "us-east-1c"]
}

variable "private_subnet_cidrs" {
  description = "CIDR blocks for private subnets"
  type        = list(string)
  default     = ["10.0.1.0/24", "10.0.2.0/24", "10.0.3.0/24"]
}

variable "public_subnet_cidrs" {
  description = "CIDR blocks for public subnets"
  type        = list(string)
  default     = ["10.0.101.0/24", "10.0.102.0/24", "10.0.103.0/24"]
}

variable "database_subnet_cidrs" {
  description = "CIDR blocks for database subnets"
  type        = list(string)
  default     = ["10.0.201.0/24", "10.0.202.0/24", "10.0.203.0/24"]
}

variable "enable_nat_gateway" {
  description = "Enable NAT Gateway for private subnets"
  type        = bool
  default     = true
}

variable "single_nat_gateway" {
  description = "Use a single NAT Gateway for all private subnets"
  type        = bool
  default     = false  # Use multiple for HA
}

# EKS Configuration
variable "kubernetes_version" {
  description = "Kubernetes version for EKS"
  type        = string
  default     = "1.28"
}

variable "eks_desired_size" {
  description = "Desired number of worker nodes"
  type        = number
  default     = 3
  validation {
    condition     = var.eks_desired_size >= 2
    error_message = "Must have at least 2 nodes for HA."
  }
}

variable "eks_min_size" {
  description = "Minimum number of worker nodes"
  type        = number
  default     = 2
}

variable "eks_max_size" {
  description = "Maximum number of worker nodes"
  type        = number
  default     = 10
}

variable "eks_instance_types" {
  description = "EC2 instance types for worker nodes"
  type        = list(string)
  default     = ["t3.xlarge", "t3.2xlarge"]
}

variable "eks_disk_size" {
  description = "Root volume size in GB for worker nodes"
  type        = number
  default     = 100
}

# RDS Configuration
variable "db_password" {
  description = "Master password for RDS database"
  type        = string
  sensitive   = true
}

variable "db_instance_class" {
  description = "RDS instance class"
  type        = string
  default     = "db.t3.large"
  validation {
    condition     = contains(["db.t3.large", "db.t3.xlarge", "db.r5.large", "db.r5.xlarge"], var.db_instance_class)
    error_message = "Use production-grade instance types."
  }
}

variable "db_allocated_storage" {
  description = "Initial allocated storage in GB"
  type        = number
  default     = 100
  validation {
    condition     = var.db_allocated_storage >= 100
    error_message = "Minimum 100 GB for production."
  }
}

variable "db_max_allocated_storage" {
  description = "Maximum allocated storage for autoscaling in GB"
  type        = number
  default     = 500
}

# Monitoring Configuration
variable "enable_monitoring" {
  description = "Enable Prometheus and Grafana monitoring"
  type        = bool
  default     = true
}

variable "grafana_admin_password" {
  description = "Grafana admin password"
  type        = string
  sensitive   = true
  default     = ""
}

variable "alarm_email_recipients" {
  description = "Email addresses for CloudWatch alarms"
  type        = list(string)
  default     = ["devops@quorumcredit.io"]
}

variable "alarm_slack_webhook_url" {
  description = "Slack webhook URL for alerts"
  type        = string
  sensitive   = true
  default     = ""
}

# CDN Configuration
variable "enable_cdn" {
  description = "Enable CloudFront CDN"
  type        = bool
  default     = false
}

variable "domain_name" {
  description = "Domain name for CloudFront"
  type        = string
  default     = "quorumcredit.xyz"
}

variable "acm_certificate_arn" {
  description = "ARN of ACM certificate for HTTPS"
  type        = string
  default     = ""
}

# Environment-specific defaults
locals {
  environment_defaults = {
    dev = {
      eks_desired_size   = 2
      eks_min_size       = 1
      eks_max_size       = 5
      eks_instance_types = ["t3.large"]
      db_instance_class  = "db.t3.small"
      enable_nat_gateway = true
      single_nat_gateway = true
    }
    staging = {
      eks_desired_size   = 3
      eks_min_size       = 2
      eks_max_size       = 8
      eks_instance_types = ["t3.xlarge"]
      db_instance_class  = "db.t3.large"
      enable_nat_gateway = true
      single_nat_gateway = false
    }
    prod = {
      eks_desired_size   = 3
      eks_min_size       = 3
      eks_max_size       = 20
      eks_instance_types = ["m5.xlarge", "m5.2xlarge"]
      db_instance_class  = "db.r5.xlarge"
      enable_nat_gateway = true
      single_nat_gateway = false
    }
  }
}
