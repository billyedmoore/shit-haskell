{-# LANGUAGE ImpredicativeTypes, TypeFamilies #-}

type family F x y

impossible :: (x ~ (forall z. F x z)) => x -> y
impossible x = x

main = pure ()
