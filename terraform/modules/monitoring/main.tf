variable "environment" { type = string }
variable "cluster_name" { type = string }
variable "enable_prometheus" { type = bool, default = true }
variable "enable_grafana" { type = bool, default = true }
variable "grafana_admin_password" { type = string, sensitive = true, default = "" }
variable "alarm_email_recipients" { type = list(string), default = [] }
variable "alarm_slack_webhook_url" { type = string, sensitive = true, default = "" }
variable "tags" { type = map(string), default = {} }
