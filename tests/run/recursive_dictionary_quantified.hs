{-# LANGUAGE FlexibleContexts, StandaloneDeriving, UndecidableInstances #-}
-- A dictionary that needs itself (bifunctors' Data.Bifunctor.Fix): Eq1 (Fix p)'s superclass
-- wants Eq (Fix p a), whose instance wants Eq (p (Fix p a) a), which Eq2 p gives back in terms
-- of Eq (Fix p a). The solver ties the knot instead of looping.
module Main where

import Data.Functor.Classes

newtype Fix p a = In { out :: p (Fix p a) a }

deriving instance Eq (p (Fix p a) a) => Eq (Fix p a)
deriving instance Show (p (Fix p a) a) => Show (Fix p a)

instance Eq2 p => Eq1 (Fix p) where
  liftEq f (In x) (In y) = liftEq2 (liftEq f) f x y

data Tree r a = Leaf a | Node r r deriving (Eq, Show)

instance Eq2 Tree where
  liftEq2 _ g (Leaf a) (Leaf b) = g a b
  liftEq2 f _ (Node a b) (Node c d) = f a c && f b d
  liftEq2 _ _ _ _ = False

instance Eq r => Eq1 (Tree r) where
  liftEq = liftEq2 (==)

leaf :: a -> Fix Tree a
leaf = In . Leaf

node :: Fix Tree a -> Fix Tree a -> Fix Tree a
node a b = In (Node a b)

main :: IO ()
main = do
  let t = node (leaf 1) (node (leaf 2) (leaf (3 :: Int)))
  print (eq1 t t, eq1 t (node (leaf 1) (leaf 2)), t == t)
  print (liftEq (\a b -> a `mod` 2 == b `mod` 2) t (node (leaf 3) (node (leaf 4) (leaf 5))))
  print t
