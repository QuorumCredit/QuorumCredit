variable "environment" { type = string }
variable "buckets" { type = any }
variable "tags" { type = map(string), default = {} }

output "logs_bucket_name" { value = "quorum-credit-logs-placeholder" }
output "backups_bucket_name" { value = "quorum-credit-backups-placeholder" }
output "artifacts_bucket_regional_domain_name" { value = "quorum-credit-artifacts-placeholder.s3.us-east-1.amazonaws.com" }
