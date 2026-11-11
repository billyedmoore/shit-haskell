{-# LANGUAGE RankNTypes, ScopedTypeVariables #-}
import Control.Monad

type GenericM m = forall a. Show a => a -> m a
type GenericQ r = forall a. Show a => a -> r
newtype GenericQ' r = GQ { unGQ :: GenericQ r }

-- Arguments bound inside the forall of a synonym (syb's style).
choiceMp :: MonadPlus m => GenericM m -> GenericM m -> GenericM m
choiceMp f g x = f x `mplus` g x

recoverMp :: MonadPlus m => GenericM m -> GenericM m
recoverMp f = f `choiceMp` return

choiceQ :: MonadPlus m => GenericQ (m r) -> GenericQ (m r) -> GenericQ (m r)
choiceQ f g x = f x `mplus` g x

applyBoth :: GenericQ String -> (Int, Bool) -> String
applyBoth q (a, b) = q a ++ "/" ++ q b

failing :: GenericM []
failing _ = []

glength :: GenericQ Int
glength = length . show

constant :: GenericQ String
constant = const "k"

-- A synonym's forall nested in itself, a lambda inside it, and a scoped type variable.
mapQ :: (forall d. Show d => d -> u) -> Int -> [u]
mapQ f n = [f n, f (show n)]

zipQ :: forall r. GenericQ (GenericQ r) -> GenericQ (GenericQ [r])
zipQ f x y = map (\q -> unGQ q y) funs
 where
  funs :: [GenericQ' r]
  funs = mapQ (\k -> GQ (f k)) 3

cmp :: (Show a, Show b) => a -> b -> Ordering
cmp x y = compare (show x) (show y) `mappend` mconcat (zipQ (\a -> cmp a) x y)

main :: IO ()
main = do
  print (choiceMp failing (\x -> [x]) (3 :: Int))
  print (recoverMp failing 'c')
  print (choiceQ (\x -> [show x]) (\x -> [show x ++ "!"]) True)
  putStrLn (applyBoth show (1, False))
  print (glength (12345 :: Int), glength "ab", constant ())
  print (zipQ (\a b -> show a ++ show b) True 'c')
  print (cmp 'a' 'b', cmp (1 :: Int) (1 :: Int))
