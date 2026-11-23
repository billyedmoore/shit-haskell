{-# LANGUAGE RankNTypes #-}

f :: forall a. forall a. a -> a
f = id

main :: IO ()
main = print (f True)
