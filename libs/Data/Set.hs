module Data.Set (
    Set, empty, singleton, fromList, toList, toAscList, toDescList, elems, insert, delete,
    member, notMember, size, null, union, unions, difference, intersection, filter, partition,
    map, foldr, foldl, findMin, findMax, lookupMin, lookupMax, isSubsetOf, deleteMin, deleteMax,
    minView, maxView, (\\), split, unions, elemAt
  ) where

import Prelude hiding (map, filter, foldr, foldl, null)
import qualified Prelude as P
import qualified Data.List as L
import Data.Set.Internal (Set(..), fromDistinctAscList, member)

infixl 9 \\

empty :: Set a
empty = Set []

singleton :: a -> Set a
singleton x = Set [x]

fromList :: Ord a => [a] -> Set a
fromList xs = Set (L.sort (L.nub xs))

toList :: Set a -> [a]
toList (Set xs) = xs

toAscList :: Set a -> [a]
toAscList (Set xs) = xs

toDescList :: Set a -> [a]
toDescList (Set xs) = P.reverse xs

elems :: Set a -> [a]
elems = toAscList

insert :: Ord a => a -> Set a -> Set a
insert x (Set xs) = Set (ins xs)
  where
    ins [] = [x]
    ins (y : ys) = case compare x y of
      LT -> x : y : ys
      EQ -> x : ys
      GT -> y : ins ys

delete :: Ord a => a -> Set a -> Set a
delete x (Set xs) = Set (P.filter (/= x) xs)

notMember :: Ord a => a -> Set a -> Bool
notMember x s = not (member x s)

size :: Set a -> Int
size (Set xs) = P.length xs

null :: Set a -> Bool
null (Set xs) = P.null xs

union :: Ord a => Set a -> Set a -> Set a
union a b = fromList (toList a ++ toList b)

unions :: Ord a => [Set a] -> Set a
unions = P.foldl union empty

difference :: Ord a => Set a -> Set a -> Set a
difference (Set xs) b = Set (P.filter (\x -> not (member x b)) xs)

(\\) :: Ord a => Set a -> Set a -> Set a
a \\ b = difference a b

intersection :: Ord a => Set a -> Set a -> Set a
intersection (Set xs) b = Set (P.filter (\x -> member x b) xs)

filter :: (a -> Bool) -> Set a -> Set a
filter p (Set xs) = Set (P.filter p xs)

partition :: (a -> Bool) -> Set a -> (Set a, Set a)
partition p s = (filter p s, filter (not . p) s)

map :: Ord b => (a -> b) -> Set a -> Set b
map f (Set xs) = fromList (P.map f xs)

foldr :: (a -> b -> b) -> b -> Set a -> b
foldr f z (Set xs) = P.foldr f z xs

foldl :: (b -> a -> b) -> b -> Set a -> b
foldl f z (Set xs) = P.foldl f z xs

findMin :: Set a -> a
findMin (Set (x : _)) = x
findMin _ = error "Set.findMin: empty set has no minimal element"

findMax :: Set a -> a
findMax (Set xs) = if P.null xs then error "Set.findMax: empty set has no maximal element" else P.last xs

lookupMin :: Set a -> Maybe a
lookupMin (Set []) = Nothing
lookupMin (Set (x : _)) = Just x

lookupMax :: Set a -> Maybe a
lookupMax (Set []) = Nothing
lookupMax (Set xs) = Just (P.last xs)

deleteMin :: Set a -> Set a
deleteMin (Set xs) = Set (P.drop 1 xs)

deleteMax :: Set a -> Set a
deleteMax (Set xs) = Set (P.reverse (P.drop 1 (P.reverse xs)))

minView :: Set a -> Maybe (a, Set a)
minView (Set []) = Nothing
minView (Set (x : xs)) = Just (x, Set xs)

maxView :: Set a -> Maybe (a, Set a)
maxView s@(Set []) = Nothing
maxView s = Just (findMax s, deleteMax s)

isSubsetOf :: Ord a => Set a -> Set a -> Bool
isSubsetOf a b = P.all (\x -> member x b) (toList a)

split :: Ord a => a -> Set a -> (Set a, Set a)
split x (Set xs) = (Set (P.filter (< x) xs), Set (P.filter (> x) xs))

elemAt :: Int -> Set a -> a
elemAt i (Set xs) = xs !! i
