{-# LANGUAGE DeriveGeneric, DeriveFoldable, DefaultSignatures, FlexibleContexts, TypeOperators #-}
import GHC.Generics

-- Count the parameter occurrences through Rep1.
class GCount f where gcount :: f p -> Int
instance GCount V1 where gcount _ = 0
instance GCount U1 where gcount _ = 0
instance GCount Par1 where gcount _ = 1
instance GCount (K1 i c) where gcount _ = 0
instance Foldable f => GCount (Rec1 f) where gcount (Rec1 x) = length x
instance (Foldable f, GCount g) => GCount (f :.: g) where gcount (Comp1 x) = foldr (\y acc -> gcount y + acc) 0 x
instance (GCount f, GCount g) => GCount (f :+: g) where
  gcount (L1 x) = gcount x
  gcount (R1 x) = gcount x
instance (GCount f, GCount g) => GCount (f :*: g) where gcount (x :*: y) = gcount x + gcount y
instance GCount f => GCount (M1 i c f) where gcount (M1 x) = gcount x

count :: (Generic1 f, GCount (Rep1 f)) => f a -> Int
count = gcount . from1

data Rose a = Rose a [Rose a] deriving (Show, Generic, Generic1, Foldable)
data T a = A | B Int a | C (Maybe a) [[a]] | D (Either String a) deriving (Show, Generic1)
newtype W a = W { unW :: (Int, a) } deriving (Show, Generic1)

-- A default method through Generic, at a parameterised type.
class Size a where
  size :: a -> Int
  default size :: (Generic a, GSize (Rep a)) => a -> Int
  size = gsize . from
class GSize f where gsize :: f p -> Int
instance GSize U1 where gsize _ = 1
instance (GSize f, GSize g) => GSize (f :+: g) where
  gsize (L1 x) = gsize x
  gsize (R1 x) = gsize x
instance (GSize f, GSize g) => GSize (f :*: g) where gsize (x :*: y) = gsize x + gsize y
instance Size c => GSize (K1 i c) where gsize (K1 x) = size x
instance GSize f => GSize (M1 i c f) where gsize (M1 x) = gsize x
instance Size Int where size _ = 1
instance Size a => Size [a] where size = sum . map size
data P a = P a a | Q a (P a) deriving (Generic)
instance Size a => Size (P a)

main :: IO ()
main = do
  print (count (Rose 'x' [Rose 'y' [], Rose 'z' []]))
  print (map count [A, B 3 'q', C (Just 'a') ["bc", "d"], D (Right 'e'), D (Left "no")])
  print (count (W (1, True)))
  print (to1 (from1 (C (Just 1) [[2, 3]])) :: T Int)
  print (unW (to1 (from1 (W (5, "five")))))
  print (size (Q (1 :: Int) (P 2 3)), size [P [1, 2 :: Int] [3]])
  print (datatypeName (from1 (Rose () [])), conName (unM1' (from1 (W (0, ())))))
  where unM1' (M1 x) = x
