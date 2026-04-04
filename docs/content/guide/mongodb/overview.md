# MongoDB Setup

Vetta stores transcripts, embeddings, and metadata in MongoDB. Two environment variables are required in every shell
session:

| Variable           | Description       | Example                                            |  
|--------------------|-------------------|----------------------------------------------------|  
| `MONGODB_URI`      | Connection string | `mongodb://localhost:27017/?directConnection=true` |  
| `MONGODB_DATABASE` | Database name     | `vetta`                                            |  

Choose the setup that fits your situation:

| Option                     | Best for                                   | Guide                                           |
|----------------------------|--------------------------------------------|-------------------------------------------------|
| **Local (Atlas CLI)**      | Development, offline work                  | [→ Local Setup](/guide/mongodb/local-atlas-cli) |
| **Atlas Cloud**            | Production, team access, cloud deployments | [→ Atlas Cloud](/guide/mongodb/atlas-cloud)     |
| **Self-Hosted / Existing** | You already have MongoDB running           | [→ Self-Hosted](/guide/mongodb/self-hosted)     |

::: warning

Both `MONGODB_URI` and `MONGODB_DATABASE` must be set before running any Vetta command. Add them to `~/.bashrc`,
`~/.zshrc`, or a `.env` file.

:::

## Initialize the Database

Once your MongoDB instance is running and your environment variables are set, you **must** initialize the database.

Vetta does not automatically create indexes when the main application starts. To ensure optimal query performance and
enforce data integrity constraints, run the dedicated migration utility from the root of the workspace:

```bash
cargo run --bin vetta_migrate
```

You should see an output similar to this:

```text
INFO Starting vetta_migrate database initialization...
INFO Environment OK. Target Database: 'vetta', URI: mongodb://localhost:27017/?directConnection=true
INFO Initializing MongoDB client...
INFO Pinging database to verify connection...
INFO Database connection verified successfully.
INFO Ensuring standard B-Tree indexes exist on collections...
INFO ✅ Standard indexes successfully verified/created.
```

::: info Note on Vector and Full-Text Search

`vetta_migrate` only applies standard MongoDB B-Tree indexes. If you are deploying to an environment that uses **Atlas
Vector Search** or **Atlas Search**, those specific indexes must be applied separately via your Infrastructure as Code (
e.g., Terraform) or the Atlas UI.

:::