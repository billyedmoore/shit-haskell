import Prelude hiding ((<>))
import Control.DeepSeq
import Control.Exception
import Text.PrettyPrint

data Tree = Leaf | Node Tree Int Tree

instance NFData Tree where
  rnf Leaf = ()
  rnf (Node l x r) = rnf l `seq` rnf x `seq` rnf r

build :: [Int] -> Tree
build = foldr ins Leaf
  where
    ins x Leaf = Node Leaf x Leaf
    ins x t@(Node l y r)
      | x < y = Node (ins x l) y r
      | x > y = Node l y (ins x r)
      | otherwise = t

doc :: Tree -> Doc
doc Leaf = text "."
doc (Node l x r) = parens (int x <+> nest 2 (sep [doc l, doc r]))

main :: IO ()
main = do
  let t = build [5, 3, 8, 1, 4, 7, 9, 2, 6]
  t `deepseq` putStrLn "forced"
  r <- try (evaluate (force [1, 2, error "boom" :: Int]))
  case r of
    Left (ErrorCall m) -> putStrLn ("caught " ++ m)
    Right v -> print v
  print (rnf (map (* 2) [1 .. 10 :: Int]))
  putStrLn (render (doc t))
  putStrLn (renderStyle style { lineLength = 30 } (doc t))
  putStrLn (render (vcat [text "a" <> colon, nest 4 (hsep (map int [1, 2, 3])), hang (text "hang") 2 (text "x" $$ text "y")]))
  print (length (show (force (replicate 3 "xyz"))))
