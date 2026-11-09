module Data.Set.Internal where

import Prelude hiding (map, filter, foldr, null)
import qualified Prelude as P

-- Sets are maps with unit values, kept in the same balanced shape.
newtype Set a = Set [a]

instance Show a => Show (Set a) where
  showsPrec d (Set xs) = showParen (d > 10) (showString "fromList " . shows xs)

instance Eq a => Eq (Set a) where
  Set a == Set b = a == b

fromDistinctAscList :: [a] -> Set a
fromDistinctAscList xs = Set xs

member :: Ord a => a -> Set a -> Bool
member x (Set xs) = P.elem x xs
