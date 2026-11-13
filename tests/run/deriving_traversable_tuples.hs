{-# LANGUAGE DeriveTraversable #-}
-- Derived Functor, Foldable and Traversable through tuples nested in lists (ShellCheck's
-- InnerToken: [(CaseType, [t], [t])] and [([t], [t])]).
module Main where

data Tok = Tok deriving (Show, Eq)

data Inner t
  = Case t [(Bool, [t], [t])]
  | If [([t], [t])] [t]
  | Co (Maybe Tok) t
  | Pair (t, Int)
  deriving (Show, Eq, Functor, Foldable, Traversable)

main :: IO ()
main = do
  let x = Case 1 [(True, [2, 3], [4]), (False, [], [5 :: Int])]
      y = If [([1], [2, 3])] [4 :: Int]
  print (fmap (* 10) x, sum x, fmap show y, length y)
  print (traverse (\v -> if v > 0 then Just (v + 1) else Nothing) x)
  print (fmap negate (Co (Just Tok) (3 :: Int)), foldr (:) [] (Pair (7 :: Int, 8)))
