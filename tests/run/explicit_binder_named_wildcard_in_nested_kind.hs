{-# LANGUAGE PartialTypeSignatures #-}

import Data.Proxy

f :: forall a. forall (b :: _k). Proxy b -> a -> a
f _ x = x

main :: IO ()
main = print (f (Proxy :: Proxy Bool) True)
