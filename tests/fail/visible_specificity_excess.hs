{-# LANGUAGE TypeApplications, ExplicitForAll #-}
f :: forall {a} b. a -> b -> (a,b)
f a b = (a,b)
main :: IO ()
main = print (f @Bool @Char 'x' True)
