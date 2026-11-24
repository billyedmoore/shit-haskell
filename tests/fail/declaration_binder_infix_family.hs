{-# LANGUAGE TypeFamilies, TypeOperators #-}
type family a :+: a
main :: IO ()
main = pure ()
