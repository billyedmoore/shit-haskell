module Data.Map (
    Map, empty, singleton, fromList, fromListWith, fromListWithKey, fromAscList, fromDistinctAscList,
    insert, insertWith, insertWithKey, delete, lookup, (!), (!?), member, notMember, findWithDefault,
    size, null, toList, toAscList, toDescList, assocs, keys, elems, keysSet,
    adjust, adjustWithKey, alter, update, union, unionWith, unionsWith, unions, intersection,
    intersectionWith, difference, (\\), filter, filterWithKey, partition, partitionWithKey,
    map, mapWithKey, mapMaybe, foldr, foldl, foldrWithKey, foldlWithKey, foldr', foldl', foldlWithKey',
    foldrWithKey', lookupMin, lookupMax, findMin, findMax, deleteMin, deleteMax, minViewWithKey,
    maxViewWithKey, minView, maxView, splitLookup, split, lookupLT, lookupGT, lookupLE, lookupGE,
    insertLookupWithKey, mapKeys, unionWithKey, restrictKeys, withoutKeys, elemAt, take, drop
  ) where

import Prelude hiding (lookup, map, filter, foldr, foldl, foldl', null, take, drop, length, elem, sum, product, maximum, minimum)
import qualified Prelude as P
import qualified Data.Set.Internal as Set

infixl 9 !, !?, \\

data Map k a = Tip | Bin Int k a (Map k a) (Map k a)

instance (Show k, Show a) => Show (Map k a) where
  showsPrec d m = showParen (d > 10) (showString "fromList " . shows (toAscList m))

instance (Eq k, Eq a) => Eq (Map k a) where
  a == b = toAscList a == toAscList b

instance Functor (Map k) where
  fmap = map

instance Foldable (Map k) where
  foldr = foldr
  foldl = foldl
  foldl' = foldl'
  toList = elems
  null = null
  length = size

size :: Map k a -> Int
size Tip = 0
size (Bin n _ _ _ _) = n

null :: Map k a -> Bool
null Tip = True
null _ = False

empty :: Map k a
empty = Tip

singleton :: k -> a -> Map k a
singleton k x = Bin 1 k x Tip Tip

delta, ratio :: Int
delta = 3
ratio = 2

bin :: k -> a -> Map k a -> Map k a -> Map k a
bin k x l r = Bin (size l + size r + 1) k x l r

balance :: k -> a -> Map k a -> Map k a -> Map k a
balance k x l r
  | sl + sr <= 1 = Bin (sl + sr + 1) k x l r
  | sr > delta * sl = rotateL k x l r
  | sl > delta * sr = rotateR k x l r
  | otherwise = Bin (sl + sr + 1) k x l r
  where
    sl = size l
    sr = size r

rotateL :: k -> a -> Map k a -> Map k a -> Map k a
rotateL k x l r@(Bin _ _ _ ly ry)
  | size ly < ratio * size ry = singleL k x l r
  | otherwise = doubleL k x l r
rotateL k x l r = bin k x l r

rotateR :: k -> a -> Map k a -> Map k a -> Map k a
rotateR k x l@(Bin _ _ _ ly ry) r
  | size ry < ratio * size ly = singleR k x l r
  | otherwise = doubleR k x l r
rotateR k x l r = bin k x l r

singleL, singleR, doubleL, doubleR :: k -> a -> Map k a -> Map k a -> Map k a
singleL k1 x1 t1 (Bin _ k2 x2 t2 t3) = bin k2 x2 (bin k1 x1 t1 t2) t3
singleL k1 x1 t1 t = bin k1 x1 t1 t
singleR k1 x1 (Bin _ k2 x2 t1 t2) t3 = bin k2 x2 t1 (bin k1 x1 t2 t3)
singleR k1 x1 t t3 = bin k1 x1 t t3
doubleL k1 x1 t1 (Bin _ k2 x2 (Bin _ k3 x3 t2 t3) t4) = bin k3 x3 (bin k1 x1 t1 t2) (bin k2 x2 t3 t4)
doubleL k1 x1 t1 t2 = singleL k1 x1 t1 t2
doubleR k1 x1 (Bin _ k2 x2 t1 (Bin _ k3 x3 t2 t3)) t4 = bin k3 x3 (bin k2 x2 t1 t2) (bin k1 x1 t3 t4)
doubleR k1 x1 t1 t2 = singleR k1 x1 t1 t2

insert :: Ord k => k -> a -> Map k a -> Map k a
insert kx x Tip = singleton kx x
insert kx x (Bin sz ky y l r) = case compare kx ky of
  LT -> balance ky y (insert kx x l) r
  GT -> balance ky y l (insert kx x r)
  EQ -> Bin sz kx x l r

insertWith :: Ord k => (a -> a -> a) -> k -> a -> Map k a -> Map k a
insertWith f = insertWithKey (\_ a b -> f a b)

insertWithKey :: Ord k => (k -> a -> a -> a) -> k -> a -> Map k a -> Map k a
insertWithKey _ kx x Tip = singleton kx x
insertWithKey f kx x (Bin sy ky y l r) = case compare kx ky of
  LT -> balance ky y (insertWithKey f kx x l) r
  GT -> balance ky y l (insertWithKey f kx x r)
  EQ -> Bin sy kx (f kx x y) l r

insertLookupWithKey :: Ord k => (k -> a -> a -> a) -> k -> a -> Map k a -> (Maybe a, Map k a)
insertLookupWithKey f kx x m = (lookup kx m, insertWithKey f kx x m)

lookup :: Ord k => k -> Map k a -> Maybe a
lookup _ Tip = Nothing
lookup k (Bin _ kx x l r) = case compare k kx of
  LT -> lookup k l
  GT -> lookup k r
  EQ -> Just x

(!) :: Ord k => Map k a -> k -> a
m ! k = case lookup k m of
  Just x -> x
  Nothing -> error "Map.!: given key is not an element in the map"

(!?) :: Ord k => Map k a -> k -> Maybe a
m !? k = lookup k m

member :: Ord k => k -> Map k a -> Bool
member k m = case lookup k m of
  Nothing -> False
  Just _ -> True

notMember :: Ord k => k -> Map k a -> Bool
notMember k m = not (member k m)

findWithDefault :: Ord k => a -> k -> Map k a -> a
findWithDefault d k m = case lookup k m of
  Nothing -> d
  Just x -> x

delete :: Ord k => k -> Map k a -> Map k a
delete _ Tip = Tip
delete k (Bin _ kx x l r) = case compare k kx of
  LT -> balance kx x (delete k l) r
  GT -> balance kx x l (delete k r)
  EQ -> glue l r

glue :: Map k a -> Map k a -> Map k a
glue Tip r = r
glue l Tip = l
glue l r
  | size l > size r = case maxViewWithKey l of
      Just ((km, m), l') -> balance km m l' r
      Nothing -> r
  | otherwise = case minViewWithKey r of
      Just ((km, m), r') -> balance km m l r'
      Nothing -> l

minViewWithKey :: Map k a -> Maybe ((k, a), Map k a)
minViewWithKey Tip = Nothing
minViewWithKey (Bin _ k x Tip r) = Just ((k, x), r)
minViewWithKey (Bin _ k x l r) = case minViewWithKey l of
  Just (km, l') -> Just (km, balance k x l' r)
  Nothing -> Nothing

maxViewWithKey :: Map k a -> Maybe ((k, a), Map k a)
maxViewWithKey Tip = Nothing
maxViewWithKey (Bin _ k x l Tip) = Just ((k, x), l)
maxViewWithKey (Bin _ k x l r) = case maxViewWithKey r of
  Just (km, r') -> Just (km, balance k x l r')
  Nothing -> Nothing

minView :: Map k a -> Maybe (a, Map k a)
minView m = case minViewWithKey m of
  Just ((_, x), m') -> Just (x, m')
  Nothing -> Nothing

maxView :: Map k a -> Maybe (a, Map k a)
maxView m = case maxViewWithKey m of
  Just ((_, x), m') -> Just (x, m')
  Nothing -> Nothing

lookupMin :: Map k a -> Maybe (k, a)
lookupMin Tip = Nothing
lookupMin (Bin _ k x Tip _) = Just (k, x)
lookupMin (Bin _ _ _ l _) = lookupMin l

lookupMax :: Map k a -> Maybe (k, a)
lookupMax Tip = Nothing
lookupMax (Bin _ k x _ Tip) = Just (k, x)
lookupMax (Bin _ _ _ _ r) = lookupMax r

findMin :: Map k a -> (k, a)
findMin m = case lookupMin m of
  Just p -> p
  Nothing -> error "Map.findMin: empty map has no minimal element"

findMax :: Map k a -> (k, a)
findMax m = case lookupMax m of
  Just p -> p
  Nothing -> error "Map.findMax: empty map has no maximal element"

deleteMin :: Map k a -> Map k a
deleteMin m = case minViewWithKey m of
  Just (_, m') -> m'
  Nothing -> Tip

deleteMax :: Map k a -> Map k a
deleteMax m = case maxViewWithKey m of
  Just (_, m') -> m'
  Nothing -> Tip

lookupLT, lookupGT, lookupLE, lookupGE :: Ord k => k -> Map k v -> Maybe (k, v)
lookupLT k m = lastMaybe [p | p@(kx, _) <- toAscList m, kx < k]
lookupGT k m = headMaybe [p | p@(kx, _) <- toAscList m, kx > k]
lookupLE k m = lastMaybe [p | p@(kx, _) <- toAscList m, kx <= k]
lookupGE k m = headMaybe [p | p@(kx, _) <- toAscList m, kx >= k]

headMaybe :: [a] -> Maybe a
headMaybe [] = Nothing
headMaybe (x : _) = Just x

lastMaybe :: [a] -> Maybe a
lastMaybe [] = Nothing
lastMaybe xs = Just (P.last xs)

adjust :: Ord k => (a -> a) -> k -> Map k a -> Map k a
adjust f = adjustWithKey (\_ x -> f x)

adjustWithKey :: Ord k => (k -> a -> a) -> k -> Map k a -> Map k a
adjustWithKey _ _ Tip = Tip
adjustWithKey f k (Bin sx kx x l r) = case compare k kx of
  LT -> Bin sx kx x (adjustWithKey f k l) r
  GT -> Bin sx kx x l (adjustWithKey f k r)
  EQ -> Bin sx kx (f kx x) l r

alter :: Ord k => (Maybe a -> Maybe a) -> k -> Map k a -> Map k a
alter f k m = case f (lookup k m) of
  Nothing -> delete k m
  Just x -> insert k x m

update :: Ord k => (a -> Maybe a) -> k -> Map k a -> Map k a
update f k m = alter (\mx -> case mx of { Nothing -> Nothing; Just x -> f x }) k m

fromList :: Ord k => [(k, a)] -> Map k a
fromList = P.foldl (\m (k, x) -> insert k x m) Tip

fromListWith :: Ord k => (a -> a -> a) -> [(k, a)] -> Map k a
fromListWith f = P.foldl (\m (k, x) -> insertWith f k x m) Tip

fromListWithKey :: Ord k => (k -> a -> a -> a) -> [(k, a)] -> Map k a
fromListWithKey f = P.foldl (\m (k, x) -> insertWithKey f k x m) Tip

fromAscList :: Ord k => [(k, a)] -> Map k a
fromAscList = fromList

fromDistinctAscList :: Ord k => [(k, a)] -> Map k a
fromDistinctAscList = fromList

toList :: Map k a -> [(k, a)]
toList = toAscList

toAscList :: Map k a -> [(k, a)]
toAscList = foldrWithKey (\k x xs -> (k, x) : xs) []

toDescList :: Map k a -> [(k, a)]
toDescList = foldlWithKey (\xs k x -> (k, x) : xs) []

assocs :: Map k a -> [(k, a)]
assocs = toAscList

keys :: Map k a -> [k]
keys = foldrWithKey (\k _ ks -> k : ks) []

elems :: Map k a -> [a]
elems = foldr (:) []

keysSet :: Map k a -> Set.Set k
keysSet m = Set.fromDistinctAscList (keys m)

restrictKeys :: Ord k => Map k a -> Set.Set k -> Map k a
restrictKeys m s = filterWithKey (\k _ -> Set.member k s) m

withoutKeys :: Ord k => Map k a -> Set.Set k -> Map k a
withoutKeys m s = filterWithKey (\k _ -> not (Set.member k s)) m

union :: Ord k => Map k a -> Map k a -> Map k a
union = unionWith const

unionWith :: Ord k => (a -> a -> a) -> Map k a -> Map k a -> Map k a
unionWith f = unionWithKey (\_ a b -> f a b)

unionWithKey :: Ord k => (k -> a -> a -> a) -> Map k a -> Map k a -> Map k a
unionWithKey f m1 m2 = foldlWithKey (\m k x -> insertWithKey (\kk new old -> f kk old new) k x m) m1 m2

unions :: Ord k => [Map k a] -> Map k a
unions = P.foldl union empty

unionsWith :: Ord k => (a -> a -> a) -> [Map k a] -> Map k a
unionsWith f = P.foldl (unionWith f) empty

intersection :: Ord k => Map k a -> Map k b -> Map k a
intersection = intersectionWith const

intersectionWith :: Ord k => (a -> b -> c) -> Map k a -> Map k b -> Map k c
intersectionWith f m1 m2 = fromDistinctAscList' [(k, f x y) | (k, x) <- toAscList m1, Just y <- [lookup k m2]]
  where fromDistinctAscList' = P.foldl (\m (k, v) -> insert k v m) Tip

difference :: Ord k => Map k a -> Map k b -> Map k a
difference m1 m2 = filterWithKey (\k _ -> notMember k m2) m1

(\\) :: Ord k => Map k a -> Map k b -> Map k a
m1 \\ m2 = difference m1 m2

filter :: (a -> Bool) -> Map k a -> Map k a
filter p = filterWithKey (\_ x -> p x)

filterWithKey :: (k -> a -> Bool) -> Map k a -> Map k a
filterWithKey p m = P.foldl (\acc (k, x) -> if p k x then insertNew k x acc else acc) Tip (toAscList m)
  where insertNew k x acc = mergeTop k x acc

-- Appends a key larger than all keys in the map.
mergeTop :: k -> a -> Map k a -> Map k a
mergeTop k x Tip = singleton k x
mergeTop k x (Bin _ ky y l r) = balance ky y l (mergeTop k x r)

partition :: (a -> Bool) -> Map k a -> (Map k a, Map k a)
partition p m = (filter p m, filter (not . p) m)

partitionWithKey :: (k -> a -> Bool) -> Map k a -> (Map k a, Map k a)
partitionWithKey p m = (filterWithKey p m, filterWithKey (\k x -> not (p k x)) m)

mapMaybe :: (a -> Maybe b) -> Map k a -> Map k b
mapMaybe f m = mapMaybeWithKey' (\_ x -> f x) m

mapMaybeWithKey' :: (k -> a -> Maybe b) -> Map k a -> Map k b
mapMaybeWithKey' f = go
  where
    go Tip = Tip
    go (Bin _ k x l r) = case f k x of
      Just y -> bin k y (go l) (go r)
      Nothing -> glue (go l) (go r)

map :: (a -> b) -> Map k a -> Map k b
map f = mapWithKey (\_ x -> f x)

mapWithKey :: (k -> a -> b) -> Map k a -> Map k b
mapWithKey _ Tip = Tip
mapWithKey f (Bin sx k x l r) = Bin sx k (f k x) (mapWithKey f l) (mapWithKey f r)

mapKeys :: Ord k2 => (k1 -> k2) -> Map k1 a -> Map k2 a
mapKeys f m = fromList [(f k, x) | (k, x) <- toAscList m]

foldr :: (a -> b -> b) -> b -> Map k a -> b
foldr f = foldrWithKey (\_ x acc -> f x acc)

foldl :: (b -> a -> b) -> b -> Map k a -> b
foldl f = foldlWithKey (\acc _ x -> f acc x)

foldr' :: (a -> b -> b) -> b -> Map k a -> b
foldr' = foldr

foldl' :: (b -> a -> b) -> b -> Map k a -> b
foldl' = foldl

foldrWithKey :: (k -> a -> b -> b) -> b -> Map k a -> b
foldrWithKey _ z Tip = z
foldrWithKey f z (Bin _ k x l r) = foldrWithKey f (f k x (foldrWithKey f z r)) l

foldlWithKey :: (b -> k -> a -> b) -> b -> Map k a -> b
foldlWithKey _ z Tip = z
foldlWithKey f z (Bin _ k x l r) = foldlWithKey f (f (foldlWithKey f z l) k x) r

foldrWithKey' :: (k -> a -> b -> b) -> b -> Map k a -> b
foldrWithKey' = foldrWithKey

foldlWithKey' :: (b -> k -> a -> b) -> b -> Map k a -> b
foldlWithKey' = foldlWithKey

split :: Ord k => k -> Map k a -> (Map k a, Map k a)
split k m = (filterWithKey (\kx _ -> kx < k) m, filterWithKey (\kx _ -> kx > k) m)

splitLookup :: Ord k => k -> Map k a -> (Map k a, Maybe a, Map k a)
splitLookup k m = let (l, r) = split k m in (l, lookup k m, r)

elemAt :: Int -> Map k a -> (k, a)
elemAt i m = toAscList m !! i

take :: Int -> Map k a -> Map k a
take n m = fromDistinctAscList' (P.take n (toAscList m))
  where fromDistinctAscList' = P.foldl (\acc (k, v) -> mergeTop k v acc) Tip

drop :: Int -> Map k a -> Map k a
drop n m = fromDistinctAscList' (P.drop n (toAscList m))
  where fromDistinctAscList' = P.foldl (\acc (k, v) -> mergeTop k v acc) Tip
