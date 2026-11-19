-- Fixity declarations for local operators (http-api-data's `where infixl 3 <!>`).
pick :: Int -> Either String (Either Int Int)
pick n = Right <$> check n <!> Left <$> check (negate n)
  where
    infixl 3 <!>
    Left _ <!> y = y
    x <!> _ = x
    check k = if k > 0 then Right k else Left "no"

main :: IO ()
main = do
  mapM_ (print . pick) [5, -2, 0]
  let infixr 2 +++
      a +++ b = "(" ++ a ++ " " ++ b ++ ")"
  putStrLn ("x" +++ "y" +++ "z" ++ "!")
