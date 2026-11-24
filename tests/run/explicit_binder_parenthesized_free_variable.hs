{-# LANGUAGE RankNTypes #-}

f :: (forall a. a -> b -> b)
f _ x = x

main :: IO ()
main = print (f True False)
