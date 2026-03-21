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
