# MongoDB Atlas (Cloud)

Best for production, team environments, and cloud-hosted Vetta instances.

## 1. Create a cluster

1. Sign up at [MongoDB Atlas](https://www.mongodb.com/products/platform/atlas-database)
2. Create a free **M0** cluster (or higher for production workloads)
3. Under **Database Access**, create a database user with read/write permissions

## 2. Configure network access

Under **Network Access**, add the IP addresses that need to reach your cluster.

Which IPs you add depends on where Vetta is running:

- **Local machine**: your public IP (Atlas offers an "Add Current IP Address" button)
- **EC2 instance**: the instance's public IP, or the NAT Gateway IP if the instance is in a private subnet
- **Dynamic environments**: consider [VPC Peering](https://www.mongodb.com/docs/atlas/security-vpc-peering/)
  or [Private Endpoints](https://www.mongodb.com/docs/atlas/security-private-endpoint/) instead of managing individual
  IPs

:::warning
Avoid using `0.0.0.0/0` (allow from anywhere) in production. Restrict access to the specific IPs or network ranges that
need connectivity to your cluster.
:::

## 3. Get your connection string

Navigate to **Connect → Drivers** and copy the `mongodb+srv://` URI.

## 4. Export environment variables

```bash
export MONGODB_URI="mongodb+srv://user:password@cluster0.xxxxx.mongodb.net/?retryWrites=true&w=majority"
export MONGODB_DATABASE="vetta"
```

## Atlas Vector Search

If you plan to use Vetta's semantic search features, create a vector search index on the `segments` collection.
See [Search & Retrieval](/technical/search-retrieval) for index definitions.