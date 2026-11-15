{-# LANGUAGE StandaloneKindSignatures, KindSignatures #-}
module SharedKinds where
import Data.Kind (Type)
type Higher :: (Type -> Type) -> Type -> Type
data Higher f a = Higher (f a)
unwrap :: Higher f a -> f a
unwrap (Higher x) = x
data Compose f g a = Compose (f (g a))
getCompose :: Compose f g a -> f (g a)
getCompose (Compose x) = x
data Wide a0 a1 a2 a3 a4 a5 a6 a7 a8 a9 a10 a11 a12 a13 a14 a15 a16 a17 a18 a19 a20 a21 a22 a23 a24 a25 a26 a27 a28 a29 a30 a31 = Wide a0 a31
edges :: Wide a0 a1 a2 a3 a4 a5 a6 a7 a8 a9 a10 a11 a12 a13 a14 a15 a16 a17 a18 a19 a20 a21 a22 a23 a24 a25 a26 a27 a28 a29 a30 a31 -> (a0, a31)
edges (Wide x y) = (x, y)
first :: Wide Int Int Int Int Int Int Int Int Int Int Int Int Int Int Int Int Int Int Int Int Int Int Int Int Int Int Int Int Int Int Int Bool
first = Wide 3 True
second :: Wide Bool Bool Bool Bool Bool Bool Bool Bool Bool Bool Bool Bool Bool Bool Bool Bool Bool Bool Bool Bool Bool Bool Bool Bool Bool Bool Bool Bool Bool Bool Bool Int
second = Wide False 4
