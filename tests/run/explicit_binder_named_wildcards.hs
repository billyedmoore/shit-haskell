{-# LANGUAGE PartialTypeSignatures #-}

f :: forall a. a -> _b -> _b
f _ x = x

main :: IO ()
main = print (f True False)
