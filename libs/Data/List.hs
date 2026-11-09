module Data.List (
    map, (++), filter, head, last, tail, init, null, length, reverse,
    and, or, any, all, concat, concatMap, foldl, foldl', foldr, foldl1, foldr1,
    sum, product, maximum, minimum, scanl, scanl1, scanr, scanr1,
    iterate, repeat, replicate, cycle, take, drop, splitAt, takeWhile, dropWhile,
    span, break, elem, notElem, lookup, zip, zip3, zipWith, zipWith3, unzip, unzip3,
    lines, words, unlines, unwords, (!!),
    sort, sortBy, sortOn, insert, insertBy, nub, nubBy, group, groupBy, partition,
    isPrefixOf, isSuffixOf, isInfixOf, intercalate, intersperse, transpose,
    find, findIndex, elemIndex, elemIndices, findIndices, delete, deleteBy,
    union, unionBy, intersect, intersectBy, (\\), tails, inits, subsequences, permutations,
    maximumBy, minimumBy, genericLength, genericTake, genericDrop, genericReplicate,
    unfoldr, stripPrefix, zip4, lookup, uncons, splitOn'
  ) where

import Base

sort :: Ord a => [a] -> [a]
sort = sortBy compare

sortOn :: Ord b => (a -> b) -> [a] -> [a]
sortOn f xs = map snd (sortBy (\a b -> compare (fst a) (fst b)) (map (\x -> let y = f x in y `seq` (y, x)) xs))

sortBy :: (a -> a -> Ordering) -> [a] -> [a]
sortBy cmp = mergeAll . sequences
  where
    sequences (a : b : xs) = case cmp a b of
      GT -> descending b [a] xs
      _ -> ascending b (\ys -> a : ys) xs
    sequences xs = [xs]
    descending a as (b : bs) = case cmp a b of
      GT -> descending b (a : as) bs
      _ -> (a : as) : sequences (b : bs)
    descending a as bs = (a : as) : sequences bs
    ascending a as (b : bs) = case cmp a b of
      GT -> let x = as [a] in x : sequences (b : bs)
      _ -> ascending b (\ys -> as (a : ys)) bs
    ascending a as bs = let x = as [a] in x : sequences bs
    mergeAll [x] = x
    mergeAll xs = mergeAll (mergePairs xs)
    mergePairs (a : b : xs) = let x = merge a b in x : mergePairs xs
    mergePairs xs = xs
    merge as@(a : as') bs@(b : bs') = case cmp a b of
      GT -> b : merge as bs'
      _ -> a : merge as' bs
    merge [] bs = bs
    merge as [] = as

insert :: Ord a => a -> [a] -> [a]
insert = insertBy compare

insertBy :: (a -> a -> Ordering) -> a -> [a] -> [a]
insertBy _ x [] = [x]
insertBy cmp x ys@(y : ys') = case cmp x y of
  GT -> y : insertBy cmp x ys'
  _ -> x : ys

nub :: Eq a => [a] -> [a]
nub = nubBy (==)

nubBy :: (a -> a -> Bool) -> [a] -> [a]
nubBy eq l = go l []
  where
    go [] _ = []
    go (x : xs) seen = if any (\y -> eq y x) seen then go xs seen else x : go xs (x : seen)

group :: Eq a => [a] -> [[a]]
group = groupBy (==)

groupBy :: (a -> a -> Bool) -> [a] -> [[a]]
groupBy _ [] = []
groupBy eq (x : xs) = let (ys, zs) = span (eq x) xs in (x : ys) : groupBy eq zs

partition :: (a -> Bool) -> [a] -> ([a], [a])
partition p xs = (filter p xs, filter (not . p) xs)

isPrefixOf :: Eq a => [a] -> [a] -> Bool
isPrefixOf [] _ = True
isPrefixOf _ [] = False
isPrefixOf (x : xs) (y : ys) = x == y && isPrefixOf xs ys

isSuffixOf :: Eq a => [a] -> [a] -> Bool
isSuffixOf xs ys = isPrefixOf (reverse xs) (reverse ys)

isInfixOf :: Eq a => [a] -> [a] -> Bool
isInfixOf needle haystack = any (isPrefixOf needle) (tails haystack)

stripPrefix :: Eq a => [a] -> [a] -> Maybe [a]
stripPrefix [] ys = Just ys
stripPrefix (x : xs) (y : ys) = if x == y then stripPrefix xs ys else Nothing
stripPrefix _ _ = Nothing

intercalate :: [a] -> [[a]] -> [a]
intercalate xs xss = concat (intersperse xs xss)

intersperse :: a -> [a] -> [a]
intersperse _ [] = []
intersperse sep (x : xs) = x : go xs
  where
    go [] = []
    go (y : ys) = sep : y : go ys

transpose :: [[a]] -> [[a]]
transpose [] = []
transpose ([] : xss) = transpose xss
transpose ((x : xs) : xss) = (x : [h | (h : _) <- xss]) : transpose (xs : [t | (_ : t) <- xss])

find :: (a -> Bool) -> [a] -> Maybe a
find p xs = case filter p xs of
  [] -> Nothing
  (y : _) -> Just y

findIndex :: (a -> Bool) -> [a] -> Maybe Int
findIndex p xs = case findIndices p xs of
  [] -> Nothing
  (i : _) -> Just i

findIndices :: (a -> Bool) -> [a] -> [Int]
findIndices p xs = [i | (x, i) <- zip xs (enumFromTo 0 (length xs - 1)), p x]

elemIndex :: Eq a => a -> [a] -> Maybe Int
elemIndex x = findIndex (== x)

elemIndices :: Eq a => a -> [a] -> [Int]
elemIndices x = findIndices (== x)

delete :: Eq a => a -> [a] -> [a]
delete = deleteBy (==)

deleteBy :: (a -> a -> Bool) -> a -> [a] -> [a]
deleteBy _ _ [] = []
deleteBy eq x (y : ys) = if eq x y then ys else y : deleteBy eq x ys

(\\) :: Eq a => [a] -> [a] -> [a]
xs \\ ys = foldl (flip delete) xs ys

union :: Eq a => [a] -> [a] -> [a]
union = unionBy (==)

unionBy :: (a -> a -> Bool) -> [a] -> [a] -> [a]
unionBy eq xs ys = xs ++ foldl (flip (deleteBy eq)) (nubBy eq ys) xs

intersect :: Eq a => [a] -> [a] -> [a]
intersect = intersectBy (==)

intersectBy :: (a -> a -> Bool) -> [a] -> [a] -> [a]
intersectBy eq xs ys = [x | x <- xs, any (eq x) ys]

tails :: [a] -> [[a]]
tails [] = [[]]
tails xs@(_ : rest) = xs : tails rest

inits :: [a] -> [[a]]
inits xs = [] : case xs of
  [] -> []
  (y : ys) -> map (y :) (inits ys)

subsequences :: [a] -> [[a]]
subsequences xs = [] : nonEmptySubsequences xs

nonEmptySubsequences :: [a] -> [[a]]
nonEmptySubsequences [] = []
nonEmptySubsequences (x : xs) = [x] : foldr (\ys r -> ys : (x : ys) : r) [] (nonEmptySubsequences xs)

permutations :: [a] -> [[a]]
permutations xs0 = xs0 : perms xs0 []
  where
    perms [] _ = []
    perms (t : ts) is' = foldr (interleave t ts) (perms ts (t : is')) (permutations is')
    interleave t ts xs r = let (_, zs) = interleave' t ts id xs r in zs
    interleave' _ ts _ [] r = (ts, r)
    interleave' t ts f (y : ys) r =
      let (us, zs) = interleave' t ts (f . (y :)) ys r
      in (y : us, f (t : y : us) : zs)

maximumBy :: (a -> a -> Ordering) -> [a] -> a
maximumBy cmp xs = foldl1 (\x y -> case cmp x y of { GT -> x; _ -> y }) xs

minimumBy :: (a -> a -> Ordering) -> [a] -> a
minimumBy cmp xs = foldl1 (\x y -> case cmp x y of { GT -> y; _ -> x }) xs

genericLength :: Num i => [a] -> i
genericLength xs = fromIntegral (length xs)

genericTake :: Integral i => i -> [a] -> [a]
genericTake n xs = take (fromIntegral n) xs

genericDrop :: Integral i => i -> [a] -> [a]
genericDrop n xs = drop (fromIntegral n) xs

genericReplicate :: Integral i => i -> a -> [a]
genericReplicate n x = replicate (fromIntegral n) x

unfoldr :: (b -> Maybe (a, b)) -> b -> [a]
unfoldr f b = case f b of
  Just (a, b') -> a : unfoldr f b'
  Nothing -> []

zip4 :: [a] -> [b] -> [c] -> [d] -> [(a, b, c, d)]
zip4 (a : as) (b : bs) (c : cs) (d : ds) = (a, b, c, d) : zip4 as bs cs ds
zip4 _ _ _ _ = []

uncons :: [a] -> Maybe (a, [a])
uncons [] = Nothing
uncons (x : xs) = Just (x, xs)

splitOn' :: Eq a => a -> [a] -> [[a]]
splitOn' d s = case break (== d) s of
  (a, []) -> [a]
  (a, _ : rest) -> a : splitOn' d rest
