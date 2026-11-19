-- UnicodeSyntax (transformers-base: `class .. ⇒ MonadBase b m | m → b where liftBase ∷ b α → m α`).
{-# LANGUAGE UnicodeSyntax, MultiParamTypeClasses, FunctionalDependencies, FlexibleInstances,
             KindSignatures, RankNTypes #-}
import Data.Kind (Type)

class (Monad b, Monad m) ⇒ MonadBase b m | m → b where
  liftBase ∷ b α → m α

instance MonadBase IO IO where
  liftBase = id

newtype Box (f ∷ ★ → ★) a = Box (f a)

applyAll ∷ ∀ a. [a → a] → a → a
applyAll fs x = foldr (\f acc → f acc) x fs

classify ∷ Int → String
classify n = case n of
  0 → "zero"
  _ | n < 0 → "negative"
    | otherwise → "positive"

main ∷ IO ()
main = do
  liftBase (putStrLn "lifted")
  xs ← return [1, 2, 3 ∷ Int]
  print [x * 2 | x ← xs]
  print (applyAll [(+ 1), (* 2)] (5 ∷ Int))
  mapM_ (putStrLn . classify) [0, -3, 7]
  case Box (Just 'c') of Box m → print m
