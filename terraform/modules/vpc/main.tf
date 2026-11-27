variable "environment" { type = string }
variable "vpc_cidr" { type = string }
variable "availability_zones" { type = list(string) }
variable "private_subnet_cidrs" { type = list(string) }
variable "public_subnet_cidrs" { type = list(string) }
variable "database_subnet_cidrs" { type = list(string) }
variable "enable_nat_gateway" { type = bool }
variable "single_nat_gateway" { type = bool }
variable "enable_dns_hostnames" { type = bool, default = true }
variable "enable_dns_support" { type = bool, default = true }
variable "tags" { type = map(string), default = {} }

output "vpc_id" { value = "vpc-placeholder" }
output "vpc_cidr" { value = var.vpc_cidr }
output "private_subnet_ids" { value = ["subnet-priv-1", "subnet-priv-2"] }
output "public_subnet_ids" { value = ["subnet-pub-1", "subnet-pub-2"] }
output "database_subnet_ids" { value = ["subnet-db-1", "subnet-db-2"] }
