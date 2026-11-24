{-# LANGUAGE RankNTypes, NoImpredicativeTypes, TypeApplications #-}
import Data.Kind (Type)
import Data.Proxy

type Poly = forall a. a -> a
bad = Proxy @Poly

main :: IO ()
main = pure ()
