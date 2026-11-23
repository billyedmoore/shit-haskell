{-# LANGUAGE TemplateHaskell, ExplicitForAll #-}
import Language.Haskell.TH

main :: IO ()
main = do
  t <- runQ [t| forall {a} b. a -> b -> (a, b) |]
  case t of
    ForallT [PlainTV _ InferredSpec, PlainTV _ SpecifiedSpec] _ _ -> putStrLn "inferred then specified"
    _ -> putStrLn "unexpected binders"
