# import os
# import voyageai
# from pymongo import MongoClient
#
# vo = voyageai.Client(api_key=os.environ["VOYAGE_API_KEY"])
# mongo = MongoClient(os.environ["MONGODB_URI"])
# collection = mongo["vetta"]["earnings_chunks"]
#
# # Step 1: Embed the query
# query_text = "What happened with Paul? Is he leaving the company?"
#
# result = vo.embed(
#     texts=[query_text],
#     model="voyage-finance-2",
#     input_type="query"  # "query" for searching!
# )
# query_embedding = result.embeddings[0]
#
# print(f"Query: {query_text}")
# print(f"Embedding dims: {len(query_embedding)}")
# print()
#
# # Step 2: Vector search
# results = collection.aggregate([
#     {
#         "$vectorSearch": {
#             "index": "chunk_vector_index",
#             "path": "embedding",
#             "queryVector": query_embedding,
#             "numCandidates": 150,
#             "limit": 5,
#             "filter": {"ticker": "MDB"}
#         }
#     },
#     {
#         "$project": {
#             "text": 1,
#             "speaker": 1,
#             "chunk_type": 1,
#             "chunk_index": 1,
#             "vs_score": {"$meta": "vectorSearchScore"}
#         }
#     }
# ])
#
# print("── Vector Search Results ──")
# print()
# for i, doc in enumerate(results, 1):
#     score = doc.get("vs_score", 0)
#     speaker = (
#             doc.get("speaker", {}).get("name")
#             or doc.get("speaker", {}).get("speaker_id", "Unknown")
#     )
#     text = doc["text"][:300]
#     print(f"{i}. [{score:.4f}] Speaker: {speaker}")
#     print(f"   {text}")
#     print()
#
# mongo.close()
