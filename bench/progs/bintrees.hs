-- Builds and walks many short-lived binary trees, keeping one long-lived tree: allocation and
-- garbage collection with a large live heap.
data Tree = Leaf | Node Tree Int Tree

build :: Int -> Int -> Tree
build v 0 = Node Leaf v Leaf
build v d = Node (build (2 * v) (d - 1)) v (build (2 * v + 1) (d - 1))

check :: Tree -> Int
check Leaf = 0
check (Node l v r) = v + check l - check r

main :: IO ()
main = do
  let keep = build 1 18
      go :: Int -> Int -> Int
      go 0 acc = acc
      go n acc = go (n - 1) (acc + check (build n 10))
  print (go 4000 0)
  print (check keep)
