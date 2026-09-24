# Terraform State Backend Configuration
# This file configures where Terraform state is stored and how it's locked

# S3 backend configuration is in main.tf as it can't use variables
# This file serves as documentation of the backend setup

# Backend Details:
# - Storage: S3 bucket (quorum-credit-terraform-state)
# - Locking: DynamoDB table (quorum-credit-tf-locks)
# - Encryption: AES256 (at-rest)
# - Versioning: Enabled
# - MFA: Optional for production

# To initialize the backend:
# 1. Create S3 bucket: aws s3api create-bucket --bucket quorum-credit-terraform-state --region us-east-1
# 2. Enable versioning: aws s3api put-bucket-versioning --bucket quorum-credit-terraform-state --versioning-configuration Status=Enabled
# 3. Enable encryption: aws s3api put-bucket-encryption --bucket quorum-credit-terraform-state --server-side-encryption-configuration '...'
# 4. Create DynamoDB table:
#    aws dynamodb create-table \
#      --table-name quorum-credit-tf-locks \
#      --attribute-definitions AttributeName=LockID,AttributeType=S \
#      --key-schema AttributeName=LockID,KeyType=HASH \
#      --billing-mode PAY_PER_REQUEST

# To migrate state from local to S3:
# 1. Initialize local backend: terraform init
# 2. Update main.tf with backend configuration
# 3. Run: terraform init -migrate-state

# To use MFA for state protection:
# aws s3api put-bucket-versioning \
#   --bucket quorum-credit-terraform-state \
#   --versioning-configuration Status=Enabled,MFADelete=Enabled \
#   --mfa "arn:aws:iam::ACCOUNT-ID:mfa/USERNAME 123456"
