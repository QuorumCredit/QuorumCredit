variable "environment" { type = string }
variable "engine" { type = string }
variable "engine_version" { type = string }
variable "db_name" { type = string }
variable "username" { type = string }
variable "password" { type = string, sensitive = true }
variable "deletion_protection" { type = bool, default = true }
variable "final_snapshot_identifier" { type = string }
variable "instance_class" { type = string }
variable "allocated_storage" { type = number }
variable "max_allocated_storage" { type = number }
variable "storage_encrypted" { type = bool, default = true }
variable "storage_type" { type = string, default = "gp3" }
variable "iops" { type = number, default = 3000 }
variable "multi_az" { type = bool, default = true }
variable "backup_retention_period" { type = number, default = 30 }
variable "backup_window" { type = string, default = "03:00-04:00" }
variable "maintenance_window" { type = string, default = "sun:04:00-sun:05:00" }
variable "vpc_id" { type = string }
variable "db_subnet_ids" { type = list(string) }
variable "allowed_security_groups" { type = list(string) }
variable "enable_cloudwatch_logs" { type = bool, default = true }
variable "log_exports" { type = list(string), default = [] }
variable "enable_performance_insights" { type = bool, default = true }
variable "performance_insights_retention_period" { type = number, default = 31 }
variable "tags" { type = map(string), default = {} }

output "db_instance_endpoint" { value = "postgres-placeholder:5432" }
output "db_instance_name" { value = var.db_name }
