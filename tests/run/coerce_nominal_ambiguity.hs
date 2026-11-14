-- `coerce` of a polymorphic function at a container's type: the source type's variable is
-- the target's element (as Set's nominal role makes it in GHC), not ambiguous.
module Main where

import Data.Coerce (coerce)
import qualified Data.Map as Map
import qualified Data.Set as Set

newtype Key = Key Int deriving (Eq, Ord, Show)

type KeySet = Set.Set Key

intersectKeys :: KeySet -> KeySet -> KeySet
intersectKeys = coerce Set.intersection

memberKey :: Key -> KeySet -> Bool
memberKey k s = Set.member k s

main :: IO ()
main = do
  let a = Set.fromList (map Key [1, 2, 3])
      b = Set.fromList (map Key [2, 3, 4])
  print (Set.toList (intersectKeys a b))
  print (Map.toList (Map.fromList [(Key 1, "a")]))
  print (coerce (Key 5) + (1 :: Int))
